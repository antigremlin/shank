use std::convert::{TryFrom, TryInto};

use serde::{Deserialize, Serialize};
use shank_macro_impl::parsed_enum::{
    ParsedEnumVariant, ParsedEnumVariantField,
};

use crate::{idl_field::IdlField, idl_type::IdlType};
use anyhow::{bail, Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum EnumFields {
    Named(Vec<IdlField>),
    Tuple(Vec<IdlType>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IdlEnumVariant {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fields: Option<EnumFields>,
}

impl TryFrom<ParsedEnumVariantField> for IdlType {
    type Error = Error;
    fn try_from(field: ParsedEnumVariantField) -> Result<Self> {
        field.rust_type.try_into()
    }
}

impl TryFrom<ParsedEnumVariant> for IdlEnumVariant {
    type Error = Error;

    fn try_from(variant: ParsedEnumVariant) -> Result<Self> {
        let mut named_fields = Vec::new();
        let mut tuple_fields = Vec::new();
        let override_ty = variant
            .idl_type_override()
            .map_err(Into::<Error>::into)?
            .map(IdlType::try_from)
            .transpose()?;

        if let Some(override_ty) = override_ty {
            if variant.fields.len() != 1 {
                bail!("idl_type override only supported on single-field variants");
            }
            let field = &variant.fields[0];
            match &field.ident {
                Some(name) => {
                    named_fields.push(IdlField {
                        name: name.to_string(),
                        ty: override_ty,
                        attrs: None,
                    });
                }
                None => tuple_fields.push(override_ty),
            }
        } else {
            for field in &variant.fields {
                let ty = IdlType::try_from(field.rust_type.clone())?;
                match &field.ident {
                    Some(name) => named_fields.push(IdlField {
                        name: name.to_string(),
                        ty,
                        attrs: None,
                    }),
                    None => tuple_fields.push(ty),
                }
            }
        }

        assert!(named_fields.is_empty() || tuple_fields.is_empty(), "should either have named or tuple fields on a variant, but never both");
        let fields = if !named_fields.is_empty() {
            Some(EnumFields::Named(named_fields))
        } else if !tuple_fields.is_empty() {
            Some(EnumFields::Tuple(tuple_fields))
        } else {
            None
        };

        Ok(Self {
            name: variant.ident.to_string(),
            fields,
        })
    }
}
