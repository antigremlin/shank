use borsh::{BorshDeserialize, BorshSerialize};
use exporting_lib::IncrementalMerkle;
use shank::{ShankAccount, ShankType};

/// This is an isolated test case for Case 2: Proxy Struct with Local Logic and Type Preservation.
///
/// We want to use the real `IncrementalMerkle` type from `exporting-lib` for our
/// account structure and program logic, but we need Shank to generate an IDL
/// that describes it properly using an imported definition.
///
/// We define a Proxy Struct that represents how the IDL should see this type.
#[derive(Clone, BorshDeserialize, BorshSerialize, ShankType)]
#[shank(import_from = "exporting-lib", rename = "IncrementalMerkle")]
pub struct MerkleTreeProxy;

/// The Outbox account data.
///
/// CRITICAL: To preserve the compiled code and data layout, we use the REAL type
/// `IncrementalMerkle` for the field. We use a Shank annotation to tell Shank
/// to treat this field as the proxy type in the IDL.
#[derive(BorshSerialize, BorshDeserialize, Debug, Default, PartialEq, Eq, ShankAccount)]
pub struct Outbox {
    /// The local domain.
    pub local_domain: u32,
    /// The bump seed of the outbox PDA.
    pub outbox_bump_seed: u8,
    /// The owner of this program.
    pub owner: Option<[u8; 32]>,

    /// The merkle tree of dispatched messages.
    /// At runtime, this is the real type from the library.
    /// In the IDL, this should be represented by MerkleTreeProxy (which imports from exporting-lib).
    #[idl_type("MerkleTreeProxy")]
    pub tree: IncrementalMerkle,

    /// Max protocol fee that can be set.
    pub max_protocol_fee: u64,
}

impl Outbox {
    /// This method demonstrates that we are using the real type and its methods.
    pub fn dispatch(&mut self, leaf: [u8; 32]) {
        self.tree.ingest(leaf);
    }
}
