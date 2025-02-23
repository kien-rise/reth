use std::hash::Hash;

use alloy_primitives::map::HashSet;

/// The implementation of forward-only in memory cursor over the entries.
///
/// The cursor operates under the assumption that the supplied collection is pre-sorted.
#[derive(Debug)]
pub struct ForwardInMemoryCursor<'a, K, V> {
    /// The reference to the pre-sorted collection of entries.
    entries: &'a [(K, V)],
    /// The index where cursor is currently positioned.
    index: usize,
    /// List of removed keys
    removed_keys: HashSet<K>,
}

impl<'a, K: Eq + Hash, V> ForwardInMemoryCursor<'a, K, V> {
    /// Create new forward cursor positioned at the beginning of the collection.
    ///
    /// The cursor expects all of the entries have been sorted in advance.
    pub fn new(entries: &'a [(K, V)], removed_keys: impl IntoIterator<Item = K>) -> Self {
        Self { entries, index: 0, removed_keys: HashSet::from_iter(removed_keys) }
    }

    /// Returns `true` if the cursor is empty, regardless of its position.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[cfg(test)]
    fn peek(&self) -> Option<&(K, V)> {
        self.entries.get(self.index)
    }
}

impl<K, V> ForwardInMemoryCursor<'_, K, V>
where
    K: PartialOrd + Clone + Eq + Hash,
    V: Clone,
{
    /// Advances the cursor until the predicate satisfies or EOF.
    fn find(&mut self, ok: impl Fn(&K) -> bool) {
        let mut step = 1usize;
        let mut halving = false;

        // We do this to make it more tolerant for debugging.
        self.index = self.index.saturating_sub(16);

        while step > 0 {
            if self.entries.get(self.index + step - 1).is_some_and(|(k, _)| !ok(k)) {
                self.index += step;
                step = if halving { step / 2 } else { step * 2 };
            } else {
                halving = true;
                step /= 2;
            }
        }
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

    /// Is key removed.
    pub fn is_removed(&self, key: &K) -> bool {
        self.removed_keys.contains(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor() {
        let mut cursor =
            ForwardInMemoryCursor::new(&[(1, ()), (2, ()), (3, ()), (4, ()), (5, ())], []);

        assert_eq!(cursor.seek(&0), Some((1, ())));
        assert_eq!(cursor.peek(), Some(&(1, ())));

        assert_eq!(cursor.seek(&3), Some((3, ())));
        assert_eq!(cursor.peek(), Some(&(3, ())));

        assert_eq!(cursor.seek(&3), Some((3, ())));
        assert_eq!(cursor.peek(), Some(&(3, ())));

        assert_eq!(cursor.seek(&4), Some((4, ())));
        assert_eq!(cursor.peek(), Some(&(4, ())));

        assert_eq!(cursor.seek(&6), None);
        assert_eq!(cursor.peek(), None);
    }
}
