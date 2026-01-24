use borsh::{BorshDeserialize, BorshSerialize};
use shank::ShankType;

/// An incremental merkle tree, modeled on the eth2 deposit contract.
/// This is the original type in the core library that we want to reuse.
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq, Eq, ShankType)]
pub struct IncrementalMerkle {
    /// The branch of the tree (32 hashes, each 32 bytes)
    pub branch: [[u8; 32]; 32],
    /// The number of leaves in the tree
    pub count: usize,
}

impl Default for IncrementalMerkle {
    fn default() -> Self {
        Self {
            branch: [[0u8; 32]; 32],
            count: 0,
        }
    }
}

impl IncrementalMerkle {
    /// Ingest a leaf into the tree.
    pub fn ingest(&mut self, _element: [u8; 32]) {
        // Real implementation would update branch and count
        self.count += 1;
    }

    /// Calculate the current tree root
    pub fn root(&self) -> [u8; 32] {
        [0u8; 32]
    }
}
