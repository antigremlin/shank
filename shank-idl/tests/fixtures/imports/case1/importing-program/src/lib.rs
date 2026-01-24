use borsh::{BorshDeserialize, BorshSerialize};
use exporting_lib::RemoteRouterConfig as RealRemoteRouterConfig;
use shank::ShankType;

/// This is a proxy struct for the RemoteRouterConfig defined in exporting-lib.
/// It is used solely to provide Shank with the metadata for IDL generation.
#[derive(Clone, BorshDeserialize, BorshSerialize, ShankType)]
#[shank(import_from = "exporting-lib", rename = "RemoteRouterConfig")]
pub struct RemoteRouterConfigProxy;

/// A local configuration struct that uses the REAL type from the library.
///
/// To preserve the compiled code and data layout, we use the real type for the field.
/// We use the #[idl_type] annotation to tell Shank to look up the definition
/// via the proxy struct defined above.
#[derive(Clone, BorshDeserialize, BorshSerialize, ShankType)]
pub struct LocalConfig {
    /// In the IDL, this should be represented by RemoteRouterConfigProxy.
    /// At runtime, this is the real type from the library.
    #[idl_type("RemoteRouterConfigProxy")]
    pub remote: RealRemoteRouterConfig,
    pub count: u64,
}

impl LocalConfig {
    /// Demonstrates usage of the real type's fields.
    pub fn get_domain(&self) -> u32 {
        self.remote.domain
    }
}
