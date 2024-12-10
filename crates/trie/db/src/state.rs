use crate::{DatabaseHashedCursorFactory, DatabaseTrieCursorFactory, PrefixSetLoader};
use alloy_primitives::{keccak256, Address, BlockNumber, Bytes, B256, U256};
use alloy_rlp::Encodable;
use reth_db::tables;
use reth_db_api::{
    cursor::DbCursorRO,
    models::{AccountBeforeTx, BlockNumberAddress},
    transaction::DbTx,
};
use reth_execution_errors::StateRootError;
use reth_storage_errors::db::DatabaseError;
use reth_trie::{
    hashed_cursor::HashedPostStateCursorFactory, trie_cursor::InMemoryTrieCursorFactory,
    updates::TrieUpdates, HashedPostState, HashedStorage, KeccakKeyHasher, KeyHasher, Nibbles,
    StateRoot, StateRootProgress, TrieInput,
};
use std::{collections::HashMap, ops::RangeInclusive};
use tracing::debug;

/// Extends [`StateRoot`] with operations specific for working with a database transaction.
pub trait DatabaseStateRoot<'a, TX>: Sized {
    /// Create a new [`StateRoot`] instance.
    fn from_tx(tx: &'a TX) -> Self;

    /// Given a block number range, identifies all the accounts and storage keys that
    /// have changed.
    ///
    /// # Returns
    ///
    /// An instance of state root calculator with account and storage prefixes loaded.
    fn incremental_root_calculator(
        tx: &'a TX,
        range: RangeInclusive<BlockNumber>,
    ) -> Result<Self, StateRootError>;

    /// Computes the state root of the trie with the changed account and storage prefixes and
    /// existing trie nodes.
    ///
    /// # Returns
    ///
    /// The updated state root.
    fn incremental_root(
        tx: &'a TX,
        range: RangeInclusive<BlockNumber>,
    ) -> Result<B256, StateRootError>;

    /// Computes the state root of the trie with the changed account and storage prefixes and
    /// existing trie nodes collecting updates in the process.
    ///
    /// Ignores the threshold.
    ///
    /// # Returns
    ///
    /// The updated state root and the trie updates.
    fn incremental_root_with_updates(
        tx: &'a TX,
        range: RangeInclusive<BlockNumber>,
    ) -> Result<(B256, TrieUpdates), StateRootError>;

    /// Computes the state root of the trie with the changed account and storage prefixes and
    /// existing trie nodes collecting updates in the process.
    ///
    /// # Returns
    ///
    /// The intermediate progress of state root computation.
    fn incremental_root_with_progress(
        tx: &'a TX,
        range: RangeInclusive<BlockNumber>,
    ) -> Result<StateRootProgress, StateRootError>;

    /// Calculate the state root for this [`HashedPostState`].
    /// Internally, this method retrieves prefixsets and uses them
    /// to calculate incremental state root.
    ///
    /// # Example
    ///
    /// ```
    /// use alloy_primitives::U256;
    /// use reth_db::test_utils::create_test_rw_db;
    /// use reth_db_api::database::Database;
    /// use reth_primitives::Account;
    /// use reth_trie::{updates::TrieUpdates, HashedPostState, StateRoot};
    /// use reth_trie_db::DatabaseStateRoot;
    ///
    /// // Initialize the database
    /// let db = create_test_rw_db();
    ///
    /// // Initialize hashed post state
    /// let mut hashed_state = HashedPostState::default();
    /// hashed_state.accounts.insert(
    ///     [0x11; 32].into(),
    ///     Some(Account { nonce: 1, balance: U256::from(10), bytecode_hash: None }),
    /// );
    ///
    /// // Calculate the state root
    /// let tx = db.tx().expect("failed to create transaction");
    /// let state_root = StateRoot::overlay_root(&tx, hashed_state);
    /// ```
    ///
    /// # Returns
    ///
    /// The state root for this [`HashedPostState`].
    fn overlay_root(tx: &'a TX, post_state: HashedPostState) -> Result<B256, StateRootError>;

    /// Calculates the state root for this [`HashedPostState`] and returns it alongside trie
    /// updates. See [`Self::overlay_root`] for more info.
    fn overlay_root_with_updates(
        tx: &'a TX,
        post_state: HashedPostState,
    ) -> Result<(B256, TrieUpdates), StateRootError>;

    /// Calculates the state root for provided [`HashedPostState`] using cached intermediate nodes.
    fn overlay_root_from_nodes(tx: &'a TX, input: TrieInput) -> Result<B256, StateRootError>;

    /// Calculates the state root and trie updates for provided [`HashedPostState`] using
    /// cached intermediate nodes.
    fn overlay_root_from_nodes_with_updates(
        tx: &'a TX,
        input: TrieInput,
    ) -> Result<(B256, TrieUpdates), StateRootError>;
}

/// Extends [`HashedPostState`] with operations specific for working with a database transaction.
pub trait DatabaseHashedPostState<TX>: Sized {
    /// Initializes [`HashedPostState`] from reverts. Iterates over state reverts from the specified
    /// block up to the current tip and aggregates them into hashed state in reverse.
    fn from_reverts<KH: KeyHasher>(tx: &TX, from: BlockNumber) -> Result<Self, DatabaseError>;
}

impl<'a, TX: DbTx> DatabaseStateRoot<'a, TX>
    for StateRoot<DatabaseTrieCursorFactory<'a, TX>, DatabaseHashedCursorFactory<'a, TX>>
{
    fn from_tx(tx: &'a TX) -> Self {
        Self::new(DatabaseTrieCursorFactory::new(tx), DatabaseHashedCursorFactory::new(tx))
    }

    fn incremental_root_calculator(
        tx: &'a TX,
        range: RangeInclusive<BlockNumber>,
    ) -> Result<Self, StateRootError> {
        let loaded_prefix_sets = PrefixSetLoader::<_, KeccakKeyHasher>::new(tx).load(range)?;
        Ok(Self::from_tx(tx).with_prefix_sets(loaded_prefix_sets))
    }

    fn incremental_root(
        tx: &'a TX,
        range: RangeInclusive<BlockNumber>,
    ) -> Result<B256, StateRootError> {
        debug!(target: "trie::loader", ?range, "incremental state root");
        Self::incremental_root_calculator(tx, range)?.root()
    }

    fn incremental_root_with_updates(
        tx: &'a TX,
        range: RangeInclusive<BlockNumber>,
    ) -> Result<(B256, TrieUpdates), StateRootError> {
        debug!(target: "trie::loader", ?range, "incremental state root");
        Self::incremental_root_calculator(tx, range)?.root_with_updates()
    }

    fn incremental_root_with_progress(
        tx: &'a TX,
        range: RangeInclusive<BlockNumber>,
    ) -> Result<StateRootProgress, StateRootError> {
        debug!(target: "trie::loader", ?range, "incremental state root with progress");
        Self::incremental_root_calculator(tx, range)?.root_with_progress()
    }

    fn overlay_root(tx: &'a TX, post_state: HashedPostState) -> Result<B256, StateRootError> {
        let prefix_sets = post_state.construct_prefix_sets().freeze();
        let state_sorted = post_state.into_sorted();
        StateRoot::new(
            DatabaseTrieCursorFactory::new(tx),
            HashedPostStateCursorFactory::new(DatabaseHashedCursorFactory::new(tx), &state_sorted),
        )
        .with_prefix_sets(prefix_sets)
        .root()
    }

    fn overlay_root_with_updates(
        tx: &'a TX,
        post_state: HashedPostState,
    ) -> Result<(B256, TrieUpdates), StateRootError> {
        let prefix_sets = post_state.construct_prefix_sets().freeze();
        let state_sorted = post_state.into_sorted();
        StateRoot::new(
            DatabaseTrieCursorFactory::new(tx),
            HashedPostStateCursorFactory::new(DatabaseHashedCursorFactory::new(tx), &state_sorted),
        )
        .with_prefix_sets(prefix_sets)
        .root_with_updates()
    }

    fn overlay_root_from_nodes(tx: &'a TX, input: TrieInput) -> Result<B256, StateRootError> {
        let state_sorted = input.state.into_sorted();
        let nodes_sorted = input.nodes.into_sorted();
        StateRoot::new(
            InMemoryTrieCursorFactory::new(DatabaseTrieCursorFactory::new(tx), &nodes_sorted),
            HashedPostStateCursorFactory::new(DatabaseHashedCursorFactory::new(tx), &state_sorted),
        )
        .with_prefix_sets(input.prefix_sets.freeze())
        .root()
    }

    fn overlay_root_from_nodes_with_updates(
        tx: &'a TX,
        input: TrieInput,
    ) -> Result<(B256, TrieUpdates), StateRootError> {
        let sequential = || {
            let input = input.clone();
            let state_sorted = input.state.into_sorted();
            let nodes_sorted = input.nodes.into_sorted();
            StateRoot::new(
                InMemoryTrieCursorFactory::new(DatabaseTrieCursorFactory::new(tx), &nodes_sorted),
                HashedPostStateCursorFactory::new(
                    DatabaseHashedCursorFactory::new(tx),
                    &state_sorted,
                ),
            )
            .with_prefix_sets(input.prefix_sets.freeze())
            .root_with_updates()
        };

        let parallel = || -> Result<(B256, TrieUpdates), StateRootError> {
            use rayon::prelude::*;

            let input = input.clone();
            let state_sorted = input.state.into_sorted();
            let nodes_sorted = input.nodes.into_sorted();

            let trie_cursor_factory =
                InMemoryTrieCursorFactory::new(DatabaseTrieCursorFactory::new(tx), &nodes_sorted);
            let hashed_cursor_factory = HashedPostStateCursorFactory::new(
                DatabaseHashedCursorFactory::new(tx),
                &state_sorted,
            );
            let prefix_sets_16 = input.prefix_sets.freeze_to_16_shards().unwrap();

            let trie_updates_16: [_; 16] = prefix_sets_16
                .into_par_iter()
                .enumerate()
                .map(|(index, prefix_sets)| {
                    StateRoot::new(trie_cursor_factory.clone(), hashed_cursor_factory.clone())
                        .with_prefix_sets(prefix_sets)
                        .root_with_updates()
                        .map(|(_state_root, mut trie_updates)| {
                            trie_updates.account_nodes.retain(|k, _v| k[0] == index as u8);
                            trie_updates.removed_nodes.retain(|k| k[0] == index as u8);
                            trie_updates.storage_tries.retain(|k, _v| (k[0] >> 4) == index as u8);
                            trie_updates
                        })
                })
                .collect::<Result<Vec<_>, _>>()?
                .try_into()
                .unwrap();

            let state_root = {
                let mut rlp_element_17: Vec<Bytes> = Vec::with_capacity(17);
                for (index, trie_updates) in trie_updates_16.iter().enumerate() {
                    let nibble = index as u8;
                    let root_hash = trie_updates
                        .account_nodes_ref()
                        .get(&Nibbles::from_nibbles(&[nibble]))
                        .and_then(|node| node.root_hash)
                        .ok_or(DatabaseError::Other(String::from("no sub root hash")))?;
                    rlp_element_17.push(root_hash.into());
                }
                rlp_element_17.push(Bytes::new());
                let mut rlp_buf = Vec::new();
                Encodable::encode(&rlp_element_17, &mut rlp_buf);
                keccak256(rlp_buf)
            };

            let mut trie_updates = TrieUpdates::default();
            for t in trie_updates_16 {
                trie_updates.extend(t);
            }

            Ok((state_root, trie_updates))
        };

        let t0 = std::time::Instant::now();
        let sequential = sequential();
        let t1 = std::time::Instant::now();
        let parallel = parallel();
        let t2 = std::time::Instant::now();

        match (&parallel, &sequential) {
            (Ok(parallel), Ok(sequential)) => {
                assert_eq!(parallel, sequential);
                println!(
                    "parallel: {:?}, sequential: {:?}",
                    t1.duration_since(t0),
                    t2.duration_since(t1)
                );
            }
            (Ok(_), Err(err)) => println!("{:?}", err),
            (Err(err), Ok(_)) => println!("{:?}", err),
            (Err(err0), Err(err1)) => println!("({:#?}, {:#?})", err0, err1),
        }
        if let (Ok(parallel), Ok(sequential)) = (&parallel, &sequential) {
            assert_eq!(parallel, sequential);
        }
        sequential
    }
}

impl<TX: DbTx> DatabaseHashedPostState<TX> for HashedPostState {
    fn from_reverts<KH: KeyHasher>(tx: &TX, from: BlockNumber) -> Result<Self, DatabaseError> {
        // Iterate over account changesets and record value before first occurring account change.
        let mut accounts = HashMap::new();
        let mut account_changesets_cursor = tx.cursor_read::<tables::AccountChangeSets>()?;
        for entry in account_changesets_cursor.walk_range(from..)? {
            let (_, AccountBeforeTx { address, info }) = entry?;
            accounts.entry(address).or_insert(info);
        }

        // Iterate over storage changesets and record value before first occurring storage change.
        let mut storages = HashMap::<Address, HashMap<B256, U256>>::default();
        let mut storage_changesets_cursor = tx.cursor_read::<tables::StorageChangeSets>()?;
        for entry in
            storage_changesets_cursor.walk_range(BlockNumberAddress((from, Address::ZERO))..)?
        {
            let (BlockNumberAddress((_, address)), storage) = entry?;
            let account_storage = storages.entry(address).or_default();
            account_storage.entry(storage.key).or_insert(storage.value);
        }

        let hashed_accounts =
            accounts.into_iter().map(|(address, info)| (KH::hash_key(address), info)).collect();

        let hashed_storages = storages
            .into_iter()
            .map(|(address, storage)| {
                (
                    KH::hash_key(address),
                    HashedStorage::from_iter(
                        // The `wiped` flag indicates only whether previous storage entries
                        // should be looked up in db or not. For reverts it's a noop since all
                        // wiped changes had been written as storage reverts.
                        false,
                        storage.into_iter().map(|(slot, value)| (KH::hash_key(slot), value)),
                    ),
                )
            })
            .collect();

        Ok(Self { accounts: hashed_accounts, storages: hashed_storages })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{hex, map::HashMap, Address, U256};
    use reth_db::test_utils::create_test_rw_db;
    use reth_db_api::database::Database;
    use reth_trie::KeccakKeyHasher;
    use revm::{db::BundleState, primitives::AccountInfo};

    #[test]
    fn from_bundle_state_with_rayon() {
        let address1 = Address::with_last_byte(1);
        let address2 = Address::with_last_byte(2);
        let slot1 = U256::from(1015);
        let slot2 = U256::from(2015);

        let account1 = AccountInfo { nonce: 1, ..Default::default() };
        let account2 = AccountInfo { nonce: 2, ..Default::default() };

        let bundle_state = BundleState::builder(2..=2)
            .state_present_account_info(address1, account1)
            .state_present_account_info(address2, account2)
            .state_storage(address1, HashMap::from_iter([(slot1, (U256::ZERO, U256::from(10)))]))
            .state_storage(address2, HashMap::from_iter([(slot2, (U256::ZERO, U256::from(20)))]))
            .build();
        assert_eq!(bundle_state.reverts.len(), 1);

        let post_state = HashedPostState::from_bundle_state::<KeccakKeyHasher>(&bundle_state.state);
        assert_eq!(post_state.accounts.len(), 2);
        assert_eq!(post_state.storages.len(), 2);

        let db = create_test_rw_db();
        let tx = db.tx().expect("failed to create transaction");
        assert_eq!(
            StateRoot::overlay_root(&tx, post_state).unwrap(),
            hex!("b464525710cafcf5d4044ac85b72c08b1e76231b8d91f288fe438cc41d8eaafd")
        );
    }
}
