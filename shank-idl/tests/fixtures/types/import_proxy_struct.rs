use shank::ShankType;

#[derive(ShankType)]
#[shank(import_from = "external", rename = "ExternalStruct")]
pub struct LocalStruct;
