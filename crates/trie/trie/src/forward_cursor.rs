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
    /// Returns the first entry from the current cursor position that's greater or equal to the
    /// provided key. This method advances the cursor forward.
    pub fn seek(&mut self, key: &K) -> Option<(K, V)> {
        if self.index > 0 {
            self.index -= 1; // for backward compatibility
        }
        let mut entry = self.entries.get(self.index);
        self.index += entry.is_some() as usize;

        while entry.is_some_and(|(k, _v)| k < key) {
            entry = self.entries.get(self.index);
            self.index += entry.is_some() as usize;
        }

        entry.cloned()
    }

    pub fn first_after(&mut self, key: &K) -> Option<(K, V)> {
        if self.index > 0 {
            self.index -= 1; // for backward compatibility
        }
        let mut entry = self.entries.get(self.index);
        self.index += entry.is_some() as usize;

        while entry.is_some_and(|(k, _v)| k <= key) {
            entry = self.entries.get(self.index);
            self.index += entry.is_some() as usize;
        }

        entry.cloned()
    }

    pub fn next(&mut self) -> Option<(K, V)> {
        let entry = self.entries.get(self.index);
        self.index += entry.is_some() as usize;
        entry.cloned()
    }
}
