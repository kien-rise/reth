//! Usage: `cargo bench --package reth-chain-state --bench trie_state`

#![allow(missing_docs)]

use std::sync::Arc;

use alloy_primitives::{Address, B256, U256};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rand::seq::SliceRandom;
use reth_chain_state::{ExecutedBlock, MemoryOverlayStateProviderRef};
use reth_primitives::{Account, NodePrimitives};
use reth_storage_api::{noop::NoopProvider, StorageRootProvider};
use reth_trie::{
    updates::{StorageTrieUpdates, TrieUpdates},
    BranchNodeCompact, HashedPostState, HashedStorage, Nibbles,
};

fn generate_hashed_post_state(
    num_updated_accounts: usize,
    num_removed_accounts: usize,
    hashed_address_choices: &[B256],
    num_storages: usize,
    num_changes_per_storage: usize,
    storage_key_choices: &[B256],
) -> HashedPostState {
    let mut rng = rand::thread_rng();

    HashedPostState {
        accounts: hashed_address_choices
            .choose_multiple(&mut rng, num_updated_accounts + num_removed_accounts)
            .enumerate()
            .map(|(i, k)| (*k, (i < num_updated_accounts).then_some(Account::default())))
            .collect(),
        storages: hashed_address_choices
            .choose_multiple(&mut rng, num_storages)
            .map(|k| {
                (
                    *k,
                    HashedStorage {
                        wiped: false,
                        storage: storage_key_choices
                            .choose_multiple(&mut rng, num_changes_per_storage)
                            .map(|k| (*k, U256::ZERO))
                            .collect(),
                    },
                )
            })
            .collect(),
    }
}

fn generate_trie_updates(
    num_updated_nodes: usize,
    num_removed_nodes: usize,
    account_nibbles_choices: &[Nibbles],
    num_storage_tries: usize,
    hashed_address_choices: &[B256],
    storage_nibbles_choices: &[Nibbles],
    num_updated_nodes_per_storage_trie: usize,
    num_removed_nodes_per_storage_trie: usize,
) -> TrieUpdates {
    let mut rng = rand::thread_rng();
    TrieUpdates {
        changed_nodes: account_nibbles_choices
            .choose_multiple(&mut rng, num_updated_nodes + num_removed_nodes)
            .enumerate()
            .map(|(i, k)| {
                (k.clone(), (i < num_updated_nodes).then_some(BranchNodeCompact::default()))
            })
            .collect(),
        storage_tries: hashed_address_choices
            .choose_multiple(&mut rng, num_storage_tries)
            .map(|k| {
                (
                    k.clone(),
                    StorageTrieUpdates {
                        is_deleted: false,
                        changed_nodes: (storage_nibbles_choices
                            .choose_multiple(
                                &mut rng,
                                num_updated_nodes_per_storage_trie +
                                    num_removed_nodes_per_storage_trie,
                            )
                            .enumerate()
                            .map(|(i, k)| {
                                (
                                    k.clone(),
                                    (i < num_updated_nodes_per_storage_trie)
                                        .then_some(BranchNodeCompact::default()),
                                )
                            })
                            .collect()),
                    },
                )
            })
            .collect(),
    }
}

fn generate_blocks<N: NodePrimitives>(len: usize) -> Vec<ExecutedBlock<N>> {
    let hashed_address_choices: Vec<_> = (0..50000).map(|_| B256::random()).collect();
    let storage_key_choices: Vec<_> = (0..4).map(|_| B256::random()).collect();
    let account_nibbles_choices: Vec<_> = (0..50000).map(|_| Nibbles::unpack(B256::random().as_slice())).collect();
    let storage_nibbles_choices: Vec<_> = (0..2).map(|_| Nibbles::unpack(B256::random().as_slice())).collect();
    (0..len)
        .map(|_| ExecutedBlock {
            recovered_block: Arc::default(),
            execution_output: Arc::default(),
            hashed_state: Arc::new(generate_hashed_post_state(
                1500, // num_updated_accounts
                0,    // num_removed_accounts
                &hashed_address_choices,
                1500, // num_storages
                2,    // num_changes_per_storage
                &storage_key_choices,
            )),
            trie: Arc::new(generate_trie_updates(
                700,  // num_updated_nodes: usize,
                0,    // num_removed_nodes: usize,
                &account_nibbles_choices,
                5000, // num_storage_tries: usize,
                &hashed_address_choices,
                &storage_nibbles_choices,
                1,    // num_updated_nodes_per_storage_trie: usize,
                0,    // num_removed_nodes_per_storage_trie: usize,
            )),
        })
        .collect()
}

#[inline]
fn run_trie_state<N: NodePrimitives>(executed_blocks: Vec<ExecutedBlock<N>>) {
    let provider =
        MemoryOverlayStateProviderRef::<N>::new(Box::new(NoopProvider::default()), executed_blocks);

    // Because `trie_state` is a private function, instead of calling it, we have to do this:
    provider.storage_root(Address::ZERO, HashedStorage::default()).unwrap();
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("run_trie_state", |b| {
        let blocks = generate_blocks(10);
        b.iter(|| run_trie_state::<reth_primitives::EthPrimitives>(black_box(blocks.clone())))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
