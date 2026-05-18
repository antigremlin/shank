use proc_macro2::Span;
use syn::{
    Attribute, Error as ParseError, Lit, Meta, MetaList, NestedMeta,
    Result as ParseResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShankImport {
    pub import_from: String,
    pub rename: Option<String>,
}

impl ShankImport {
    pub fn from_attrs(attrs: &[Attribute]) -> ParseResult<Option<Self>> {
        let shank_attrs: Vec<&Attribute> =
            attrs.iter().filter(|attr| attr.path.is_ident("shank")).collect();

        if shank_attrs.is_empty() {
            return Ok(None);
        }

        if shank_attrs.len() > 1 {
            return Err(ParseError::new(
                Span::call_site(),
                "Only one #[shank(...)] attribute is allowed per item",
            ));
        }

        let meta = shank_attrs[0].parse_meta()?;
        let nested = match meta {
            Meta::List(MetaList { nested, .. }) => nested,
            _ => {
                return Err(ParseError::new(
                    Span::call_site(),
                    "Expected #[shank(...)] with import_from = \"...\"",
                ))
            }
        };

        let mut import_from: Option<String> = None;
        let mut rename: Option<String> = None;

        for item in nested.iter() {
            match item {
                NestedMeta::Meta(Meta::NameValue(name_value)) => {
                    let ident = name_value
                        .path
                        .get_ident()
                        .ok_or_else(|| {
                            ParseError::new(
                                Span::call_site(),
                                "Expected simple identifiers in #[shank(...)]",
                            )
                        })?
                        .to_string();
                    let value = match &name_value.lit {
                        Lit::Str(lit) => lit.value(),
                        _ => {
                            return Err(ParseError::new(
                                name_value.lit.span(),
                                "Expected string literal in #[shank(...)]",
                            ))
                        }
                    };

                    match ident.as_str() {
                        "import_from" => {
                            if import_from.is_some() {
                                return Err(ParseError::new(
                                    Span::call_site(),
                                    "Duplicate import_from in #[shank(...)]",
                                ));
                            }
                            import_from = Some(value);
                        }
                        "rename" => {
                            if rename.is_some() {
                                return Err(ParseError::new(
                                    Span::call_site(),
                                    "Duplicate rename in #[shank(...)]",
                                ));
                            }
                            rename = Some(value);
                        }
                        _ => {
                            return Err(ParseError::new(
                                Span::call_site(),
                                format!(
                                    "Unknown #[shank] attribute '{}'. Supported: import_from, rename",
                                    ident
                                ),
                            ))
                        }
                    }
                }
                _ => {
                    return Err(ParseError::new(
                        Span::call_site(),
                        "Expected name-value pairs in #[shank(...)]",
                    ))
                }
            }
        }

        let import_from = import_from.ok_or_else(|| {
            ParseError::new(
                Span::call_site(),
                "Missing import_from in #[shank(...)]",
            )
        })?;

        Ok(Some(Self { import_from, rename }))
    }
}
