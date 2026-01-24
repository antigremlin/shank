use borsh::{BorshDeserialize, BorshSerialize};
use shank::ShankType;

/// Instruction data for initializing the program.
#[derive(BorshDeserialize, BorshSerialize, Debug, PartialEq, ShankType)]
pub struct Init {
    /// The address of the mailbox contract.
    pub mailbox: [u8; 32],
    /// The local decimals.
    pub decimals: u8,
}

/// Instruction data for transferring tokens.
#[derive(BorshDeserialize, BorshSerialize, Debug, PartialEq, ShankType)]
pub struct TransferRemote {
    /// The destination domain.
    pub destination_domain: u32,
    /// The remote recipient.
    pub recipient: [u8; 32],
}

/// Instructions shared by all Hyperlane Sealevel Token programs.
/// This is the enum we want the program to "re-export" via its IDL.
#[derive(BorshDeserialize, BorshSerialize, Debug, PartialEq)]
pub enum Instruction {
    /// Initialize the program.
    Init(Init),
    /// Transfer tokens to a remote recipient.
    TransferRemote(TransferRemote),
}
