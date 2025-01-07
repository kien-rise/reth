use std::{collections::BTreeMap, ops::RangeBounds, sync::Arc};

use parking_lot::Mutex;
use reth_db::{
    common::{KeyValue, PairResult, ValueOnlyResult},
    cursor::{DbCursorRO, DbDupCursorRO, RangeWalker, ReverseWalker, Walker},
    table::{DupSort, Table},
    transaction::DbTx,
    DatabaseError,
};

#[derive(Debug)]
pub struct CacheTable<T: Table> {
    seek: Mutex<BTreeMap<<T as Table>::Key, Option<KeyValue<T>>>>,
    seek_exact: Mutex<BTreeMap<<T as Table>::Key, Option<KeyValue<T>>>>,
    next: Mutex<BTreeMap<<T as Table>::Key, Option<KeyValue<T>>>>,
}

impl<T: Table> Default for CacheTable<T> {
    fn default() -> Self {
        Self {
            seek: Mutex::default(),
            seek_exact: Mutex::default(),
            next: Mutex::default(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Cache {
    accounts_trie: Arc<CacheTable<reth_db::AccountsTrie>>,
    hashed_accounts: Arc<CacheTable<reth_db::HashedAccounts>>,
}

static LOGS: std::sync::OnceLock<(
    dashmap::DashMap<String, usize>,
    std::sync::atomic::AtomicUsize,
)> = std::sync::OnceLock::new();

fn log(text: &str) {
    let (logs, count) = LOGS.get_or_init(|| Default::default());
    let mut entry = logs.entry(String::from(text)).or_default();
    *entry.value_mut() += 1;
    drop(entry);
    let count = count.fetch_add(1, std::sync::atomic::Ordering::Acquire) + 1;
    if count.is_power_of_two() {
        println!("count = {}", count);
        for entry in logs.iter() {
            println!("{:10} {}", entry.value(), entry.key());
        }
    }
}

macro_rules! log {
    ($($arg:tt)*) => {
        log(&format!($($arg)*));
    };
}

impl Cache {
    pub fn table<T: Table>(&self) -> Option<Arc<CacheTable<T>>> {
        if let Ok(table) = Arc::downcast(self.accounts_trie.clone()) {
            return Some(table);
        }
        if let Ok(table) = Arc::downcast(self.hashed_accounts.clone()) {
            return Some(table);
        }
        log!("table not cached | {}", T::NAME);
        None
    }
}

pub(crate) struct CacheCursor<T: Table, C> {
    cursor: C,
    position: Option<<T as Table>::Key>,
    cursor_in_sync: bool,
    cache_table: Option<Arc<CacheTable<T>>>,
}

impl<T: Table, C: DbCursorRO<T>> DbCursorRO<T> for CacheCursor<T, C> {
    fn first(&mut self) -> PairResult<T> {
        unreachable!()
    }

    fn seek_exact(&mut self, key: <T as Table>::Key) -> PairResult<T> {
        if let Some(cache) = self.cache_table.as_ref() {
            if let Some(cached_value) = cache.seek_exact.lock().get(&key).cloned() {
                self.position = cached_value.as_ref().map(|(k, _v)| k.clone());
                self.cursor_in_sync = false;
                return Ok(cached_value);
            }
        }

        let result = self.cursor.seek_exact(key.clone())?;

        self.position = result.as_ref().map(|(k, _v)| k.clone());
        if let Some(cache) = self.cache_table.as_ref() {
            cache.seek_exact.lock().insert(key, result.clone());
        }

        Ok(result)
    }

    fn seek(&mut self, key: <T as Table>::Key) -> PairResult<T> {
        if let Some(cache) = self.cache_table.as_ref() {
            if let Some(cached_value) = cache.seek.lock().get(&key).cloned() {
                self.position = cached_value.as_ref().map(|(k, _v)| k.clone());
                self.cursor_in_sync = false;
                return Ok(cached_value);
            }
        }

        let result = self.cursor.seek(key.clone())?;

        self.position = result.as_ref().map(|(k, _v)| k.clone());
        self.cursor_in_sync = true;
        if let Some(cache) = self.cache_table.as_ref() {
            cache.seek.lock().insert(key, result.clone());
        }

        Ok(result)
    }

    fn next(&mut self) -> PairResult<T> {
        let Some(key) = self.position.as_ref().cloned() else { return Ok(None) };

        if let Some(cache) = &self.cache_table {
            if let Some(cached_value) = cache.next.lock().get(&key).cloned() {
                self.position = cached_value.as_ref().map(|(k, _v)| k.clone());
                self.cursor_in_sync = false;
                return Ok(cached_value);
            }
        }

        let result = if self.cursor_in_sync {
            self.cursor.next()?
        } else {
            self.cursor.seek(key.clone())?;
            self.cursor.next()?
        };

        self.position = result.as_ref().map(|(k, _v)| k.clone());
        self.cursor_in_sync = true;
        if let Some(cache) = self.cache_table.as_ref() {
            cache.next.lock().insert(key, result.clone());
        }

        Ok(result)
    }

    fn prev(&mut self) -> PairResult<T> {
        unreachable!()
    }

    fn last(&mut self) -> PairResult<T> {
        unreachable!()
    }

    fn current(&mut self) -> PairResult<T> {
        unreachable!()
    }

    fn walk(
        &mut self,
        _start_key: Option<<T as Table>::Key>,
    ) -> Result<Walker<'_, T, Self>, DatabaseError>
    where
        Self: Sized,
    {
        unreachable!()
    }

    fn walk_range(
        &mut self,
        _range: impl RangeBounds<<T as Table>::Key>,
    ) -> Result<RangeWalker<'_, T, Self>, DatabaseError>
    where
        Self: Sized,
    {
        unreachable!()
    }

    fn walk_back(
        &mut self,
        _start_key: Option<<T as Table>::Key>,
    ) -> Result<ReverseWalker<'_, T, Self>, DatabaseError>
    where
        Self: Sized,
    {
        unreachable!()
    }
}

pub(crate) struct CacheDupCursor<T: Table, C> {
    cursor: C,
    position: Option<<T as Table>::Key>,
    cache_table: Option<Arc<CacheTable<T>>>,
}

impl<T: Table, C: DbCursorRO<T>> DbCursorRO<T> for CacheDupCursor<T, C> {
    fn first(&mut self) -> PairResult<T> {
        unreachable!()
    }

    fn seek_exact(&mut self, key: <T as Table>::Key) -> PairResult<T> {
        if let Some(cache) = self.cache_table.as_ref() {
            if let Some(cached_value) = cache.seek_exact.lock().get(&key).cloned() {
                return Ok(cached_value);
            }
        }

        let result = self.cursor.seek_exact(key.clone())?;

        self.position = result.as_ref().map(|(k, _v)| k.clone());
        if let Some(cache) = self.cache_table.as_ref() {
            cache.seek_exact.lock().insert(key, result.clone());
        }

        Ok(result)
    }

    fn seek(&mut self, _key: <T as Table>::Key) -> PairResult<T> {
        unreachable!()
    }

    fn next(&mut self) -> PairResult<T> {
        unreachable!()
    }

    fn prev(&mut self) -> PairResult<T> {
        unreachable!()
    }

    fn last(&mut self) -> PairResult<T> {
        unreachable!()
    }

    fn current(&mut self) -> PairResult<T> {
        unreachable!()
    }

    fn walk(
        &mut self,
        _start_key: Option<<T as Table>::Key>,
    ) -> Result<reth_db::cursor::Walker<'_, T, Self>, DatabaseError>
    where
        Self: Sized,
    {
        unreachable!()
    }

    fn walk_range(
        &mut self,
        _range: impl RangeBounds<<T as Table>::Key>,
    ) -> Result<reth_db::cursor::RangeWalker<'_, T, Self>, DatabaseError>
    where
        Self: Sized,
    {
        unreachable!()
    }

    fn walk_back(
        &mut self,
        _start_key: Option<<T as Table>::Key>,
    ) -> Result<ReverseWalker<'_, T, Self>, DatabaseError>
    where
        Self: Sized,
    {
        unreachable!()
    }
}

impl<T: Table + DupSort, C: DbDupCursorRO<T>> DbDupCursorRO<T> for CacheDupCursor<T, C> {
    fn next_dup(&mut self) -> PairResult<T> {
        log!("CacheDupCursor::next_dup | {}", T::NAME);
        self.cursor.next_dup()
    }

    fn next_no_dup(&mut self) -> PairResult<T> {
        unreachable!()
    }

    fn next_dup_val(&mut self) -> ValueOnlyResult<T> {
        log!("CacheDupCursor::next_dup_val | {}", T::NAME);
        self.cursor.next_dup_val()
    }

    fn seek_by_key_subkey(
        &mut self,
        key: <T>::Key,
        subkey: <T as DupSort>::SubKey,
    ) -> ValueOnlyResult<T> {
        log!("CacheDupCursor::seek_by_key_subkey | {}", T::NAME);
        self.cursor.seek_by_key_subkey(key, subkey)
    }

    fn walk_dup(
        &mut self,
        _key: Option<<T>::Key>,
        _subkey: Option<<T as DupSort>::SubKey>,
    ) -> Result<reth_db::cursor::DupWalker<'_, T, Self>, DatabaseError> {
        unreachable!()
    }
}

#[derive(Debug)]
pub(crate) struct CacheTx<'a, Tx> {
    pub(crate) tx: &'a Tx,
    pub(crate) cache: Option<&'a Cache>,
}

impl<'a, Tx: DbTx> DbTx for CacheTx<'a, Tx> {
    type Cursor<T: Table> = CacheCursor<T, Tx::Cursor<T>>;

    type DupCursor<T: DupSort> = CacheDupCursor<T, Tx::DupCursor<T>>;

    fn get<T: Table>(&self, key: T::Key) -> Result<Option<T::Value>, DatabaseError> {
        log!("CacheTx::get | {}", T::NAME);
        self.tx.get::<T>(key)
    }

    fn get_by_encoded_key<T: Table>(
        &self,
        _key: &<T::Key as reth_db::table::Encode>::Encoded,
    ) -> Result<Option<T::Value>, DatabaseError> {
        unreachable!();
    }

    fn commit(self) -> Result<bool, DatabaseError> {
        unreachable!()
    }

    fn abort(self) {
        unreachable!()
    }

    fn cursor_read<T: Table>(&self) -> Result<Self::Cursor<T>, reth_db::DatabaseError> {
        Ok(CacheCursor {
            cursor: self.tx.cursor_read()?,
            position: None,
            cursor_in_sync: false,
            cache_table: self.cache.and_then(|cache| cache.table::<T>()),
        })
    }

    fn cursor_dup_read<T: reth_db::table::DupSort>(
        &self,
    ) -> Result<Self::DupCursor<T>, reth_db::DatabaseError> {
        Ok(CacheDupCursor {
            cursor: self.tx.cursor_dup_read()?,
            position: None,
            cache_table: self.cache.and_then(|cache| cache.table::<T>()),
        })
    }

    fn entries<T: Table>(&self) -> Result<usize, reth_db::DatabaseError> {
        unreachable!();
    }

    fn disable_long_read_transaction_safety(&mut self) {
        unreachable!()
    }
}
