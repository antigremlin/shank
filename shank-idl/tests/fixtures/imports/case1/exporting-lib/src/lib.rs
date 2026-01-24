use borsh::{BorshDeserialize, BorshSerialize};
use shank::ShankType;

/// Configuration for a remote router.
/// This is the original type that will be exported via IDL.
#[derive(Debug, Clone, PartialEq, BorshDeserialize, BorshSerialize, ShankType)]
pub struct RemoteRouterConfig {
    /// The domain of the remote router.
    pub domain: u32,
    /// The remote router address.
    pub router: Option<[u8; 32]>,
}
