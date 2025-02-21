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
/// The implementation of forward-only in memory cursor over the entries.
/// The cursor operates under the assumption that the supplied collection is pre-sorted.
#[derive(Debug)]
pub struct ForwardInMemoryCursor<'a, K, V> {
    /// The reference to the pre-sorted collection of entries.
    entries: &'a Vec<(K, V)>,
    /// The index where cursor is currently positioned.
    index: usize,
}

impl<'a, K, V> ForwardInMemoryCursor<'a, K, V> {
    /// Create new forward cursor positioned at the beginning of the collection.
    /// The cursor expects all of the entries have been sorted in advance.
    pub const fn new(entries: &'a Vec<(K, V)>) -> Self {
        Self { entries, index: 0 }
    }

    /// Returns `true` if the cursor is empty, regardless of its position.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<K, V> ForwardInMemoryCursor<'_, K, V>
where
    K: PartialOrd + Clone,
    V: Clone,
{
    /// Advances the cursor until the predicate satisfies or EOF.
    fn find(&mut self, ok: impl Fn(&K) -> bool) {
        let initial = self.index;

        let mut expected = self.index;
        while self.entries.get(expected).is_some_and(|(k, _)| !ok(k)) {
            expected += 1;
        }

        let mut step = 1usize;
        let mut halving = false;
        while step > 0 {
            if self.entries.get(self.index + step - 1).is_some_and(|(k, _)| !ok(k)) {
                self.index += step;
                step = if halving { step / 2 } else { step * 2 };
            } else {
                halving = true;
                step /= 2;
            }
        }

        assert_eq!(expected, self.index);
        log!("distance = {:?}", self.index - initial);
    }

    /// Returns the first entry from the current cursor position that's greater or equal to the
    /// provided key. This method advances the cursor forward.
    pub fn seek(&mut self, key: &K) -> Option<(K, V)> {
        self.find(|k| k >= key);
        self.entries.get(self.index).cloned()
    }

    /// Returns the first entry from the current cursor position that's greater than the provided
    /// key. This method advances the cursor forward.
    pub fn next(&mut self, key: &K) -> Option<(K, V)> {
        self.find(|k| k > key);
        self.entries.get(self.index).cloned()
    }
}
