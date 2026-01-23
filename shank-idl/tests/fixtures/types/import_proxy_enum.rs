use shank::ShankType;

#[derive(ShankType)]
#[shank(import_from = "external", rename = "ExternalEnum")]
pub enum LocalEnum {}
