use std::sync::Arc;

use crate::{prefix_set::TriePrefixSetsMut, updates::TrieUpdates, HashedPostState};

/// Inputs for trie-related computations.
#[derive(Default, Debug, Clone)]
pub struct TrieInput {
    /// The collection of cached in-memory intermediate trie nodes that
    /// can be reused for computation.
    pub nodes: Vec<Arc<TrieUpdates>>, // oldest to newest
    /// The in-memory overlay hashed state.
    pub state: Vec<Arc<HashedPostState>>, // oldest to newest
    /// The collection of prefix sets for the computation. Since the prefix sets _always_
    /// invalidate the in-memory nodes, not all keys from `self.state` might be present here,
    /// if we have cached nodes for them.
    pub prefix_sets: TriePrefixSetsMut,
}

impl TrieInput {
    /// Create new trie input.
    pub const fn new(
        nodes: Vec<Arc<TrieUpdates>>,
        state: Vec<Arc<HashedPostState>>,
        prefix_sets: TriePrefixSetsMut,
    ) -> Self {
        Self { nodes, state, prefix_sets }
    }

    /// Create new trie input from in-memory state. The prefix sets will be constructed and
    /// set automatically.
    pub fn from_state(state: Arc<HashedPostState>) -> Self {
        let prefix_sets = state.construct_prefix_sets();
        Self { nodes: Vec::default(), state: vec![state], prefix_sets }
    }

    /// Prepend state to the input and extend the prefix sets.
    pub fn prepend(&mut self, state: Arc<HashedPostState>) {
        self.prefix_sets.extend(state.construct_prefix_sets());
        self.state.insert(0, state);
    }

    /// Prepend intermediate nodes and state to the input.
    /// Prefix sets for incoming state will be ignored.
    pub fn prepend_cached(&mut self, nodes: Vec<Arc<TrieUpdates>>, state: Vec<Arc<HashedPostState>>) {
        self.nodes.splice(0..0, nodes);
        self.state.splice(0..0, state);
    }

    /// Append state to the input and extend the prefix sets.
    pub fn append(&mut self, state: Arc<HashedPostState>) {
        self.prefix_sets.extend(state.construct_prefix_sets());
        self.state.push(state);
    }

    /// Append intermediate nodes and state to the input.
    /// Prefix sets for incoming state will be ignored.
    pub fn append_cached(&mut self, nodes: Arc<TrieUpdates>, state: Arc<HashedPostState>) {
        self.nodes.push(nodes);
        self.state.push(state);
    }
}
