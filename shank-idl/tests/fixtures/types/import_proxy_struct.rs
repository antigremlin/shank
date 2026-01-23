use shank::ShankType;

#[derive(ShankType)]
#[shank(import_from = "tests/fixtures/types/idl/external.json", rename = "ExternalStruct")]
pub struct LocalStruct;
