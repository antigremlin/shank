use borsh::{BorshDeserialize, BorshSerialize};
use exporting_lib::Instruction as RealInstruction;
use shank::{ShankInstruction, ShankType};

/// This is an isolated test case for Case 3: Shared Instruction Enum with Type Preservation.
///
/// The Hyperlane Sealevel Token programs rely on a shared instruction set defined
/// in a common library (`exporting-lib` in this test). We want to use the REAL
/// enum for processing, but tell Shank to import the definition for the IDL.

/// This is a proxy enum for the shared Instruction defined in exporting-lib.
/// It provides Shank with the metadata needed to import the enum variants and
/// their associated data types from the library's IDL.
#[derive(Debug, BorshDeserialize, BorshSerialize, ShankInstruction)]
#[shank(import_from = "exporting-lib", rename = "Instruction")]
pub enum TokenInstructionProxy {}

/// Supporting types used by the shared instructions also need proxies
/// if they are to be resolved correctly in the generated IDL.

#[derive(BorshDeserialize, BorshSerialize, ShankType)]
#[shank(import_from = "exporting-lib", rename = "Init")]
pub struct InitProxy;

#[derive(BorshDeserialize, BorshSerialize, ShankType)]
#[shank(import_from = "exporting-lib", rename = "TransferRemote")]
pub struct TransferRemoteProxy;

/// A wrapper or a processor that uses the REAL instruction enum.
/// While Shank generates the IDL based on the Proxy, the actual program
/// logic operates on `RealInstruction`.
pub struct InstructionProcessor;

impl InstructionProcessor {
    pub fn process(data: &[u8]) -> Result<(), std::io::Error> {
        let _ix = RealInstruction::deserialize(&mut &data[..])?;
        // Process the real instruction...
        Ok(())
    }
}

/// Example of a program-specific instruction set.
/// If this program wanted to wrap the library instructions, it might do this:
#[derive(Debug, BorshDeserialize, BorshSerialize, ShankInstruction)]
pub enum LocalExtension {
    /// An instruction unique to this specific token implementation.
    Custom(u64),

    /// In a scenario where we wrap the external instruction, we'd use the proxy for IDL.
    #[idl_type("TokenInstructionProxy")]
    Base(RealInstruction),
}
