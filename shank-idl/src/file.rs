use anyhow::{bail, Context, Result};

use std::{
    collections::{HashMap, HashSet},
    convert::{TryFrom, TryInto},
    path::{Path, PathBuf},
};

use crate::{
    idl::{Idl, IdlConst, IdlEvent, IdlState},
    idl_error_code::IdlErrorCode,
    idl_instruction::{IdlInstruction, IdlInstructions},
    idl_metadata::IdlMetadata,
    idl_type::IdlType,
    idl_type_definition::IdlTypeDefinition,
    manifest::{Manifest, WithPath},
};
use shank_macro_impl::{
    account::extract_account_structs,
    converters::parse_error_into,
    custom_type::{CustomEnum, CustomStruct, DetectCustomTypeConfig},
    error::{ProgramErrors, DERIVE_THIS_ERROR_ATTR},
    instruction::Instruction,
    krate::CrateContext,
    macros::ProgramId,
    parsed_enum::ParsedEnum,
    parsed_struct::{ParsedStruct, StructAttr},
    parsers::get_derive_attr,
    syn::Item,
    syn::{Expr, ExprLit, Lit},
    DERIVE_ACCOUNT_ATTR,
    shank_import::ShankImport,
    DERIVE_INSTRUCTION_ATTR,
    types::with_const_lengths,
};

// -----------------
// ParseIdlConfig
// -----------------
#[derive(Debug)]
pub struct ParseIdlConfig {
    pub program_version: String,
    pub program_name: String,
    pub detect_custom_struct: DetectCustomTypeConfig,
    pub require_program_address: bool,
    pub program_address_override: Option<String>,
}

impl Default for ParseIdlConfig {
    fn default() -> Self {
        Self {
            program_version: Default::default(),
            program_name: Default::default(),
            detect_custom_struct: Default::default(),
            require_program_address: true,
            program_address_override: None,
        }
    }
}

impl ParseIdlConfig {
    pub fn optional_program_address() -> Self {
        Self {
            require_program_address: false,
            ..Self::default()
        }
    }
}

type ImportCache = HashMap<PathBuf, Idl>;
type TypeImportMap = HashMap<String, HashMap<String, String>>;

struct ImportContext {
    root: PathBuf,
    manifest: Option<WithPath<Manifest>>,
}

fn structs_with_paths(
    ctx: &CrateContext,
) -> Vec<(PathBuf, shank_macro_impl::syn::ItemStruct)> {
    let mut items = Vec::new();
    for module in ctx.modules() {
        let file = module.detail.file.clone();
        for item in &module.detail.items {
            if let Item::Struct(strct) = item {
                items.push((file.clone(), strct.clone()));
            }
        }
    }
    items
}

fn enums_with_paths(
    ctx: &CrateContext,
) -> Vec<(PathBuf, shank_macro_impl::syn::ItemEnum)> {
    let mut items = Vec::new();
    for module in ctx.modules() {
        let file = module.detail.file.clone();
        for item in &module.detail.items {
            if let Item::Enum(enm) = item {
                items.push((file.clone(), enm.clone()));
            }
        }
    }
    items
}

// -----------------
// Parse File
// -----------------

/// Parse an entire interface file.
pub fn parse_file(
    filename: impl AsRef<Path>,
    config: &ParseIdlConfig,
) -> Result<Option<Idl>> {
    let filename = filename.as_ref();
    let ctx = CrateContext::parse(filename)?;
    let import_ctx = resolve_import_context(filename)?;
    let const_lengths = collect_const_lengths(&ctx);
    with_const_lengths(const_lengths, || {
        let mut import_cache: ImportCache = HashMap::new();

        let constants = constants(&ctx)?;
        let state = state(&ctx)?;
        let accounts = accounts(&ctx)?;
        let (types, type_imports) = types(
            &ctx,
            &config.detect_custom_struct,
            &import_ctx,
            &mut import_cache,
        )?;
        let instructions = instructions(
            &ctx,
            &import_ctx,
            &type_imports,
            &mut import_cache,
        )?;
        let events = events(&ctx)?;
        let errors = errors(&ctx)?;
        let metadata = metadata(
            &ctx,
            config.require_program_address,
            config.program_address_override.as_ref(),
        )?;

        let mut idl = Idl {
            version: config.program_version.to_string(),
            name: config.program_name.to_string(),
            constants,
            instructions,
            state,
            accounts,
            types,
            events,
            errors,
            metadata,
        };

        // Populate sentinel values for PodOption<CustomType> fields from type definitions
        populate_pod_option_sentinels(&mut idl)?;

        // Validate that custom types used in PodOption have pod_sentinel defined
        validate_pod_option_sentinels(&idl)?;

        Ok(Some(idl))
    })
}

fn accounts(ctx: &CrateContext) -> Result<Vec<IdlTypeDefinition>> {
    let mut accounts: Vec<IdlTypeDefinition> = Vec::new();
    for (file, strct) in structs_with_paths(ctx) {
        if get_derive_attr(&strct.attrs, DERIVE_ACCOUNT_ATTR).is_none() {
            continue;
        }
        let parsed = extract_account_structs(std::iter::once(&strct))
            .with_context(|| {
                format!(
                    "While parsing ShankAccount '{}' in {}",
                    strct.ident,
                    file.display()
                )
            })?;
        for parsed_struct in parsed {
            let idl_def: IdlTypeDefinition = parsed_struct.try_into()?;
            accounts.push(idl_def);
        }
    }
    Ok(accounts)
}

fn instructions(
    ctx: &CrateContext,
    import_ctx: &ImportContext,
    type_imports: &TypeImportMap,
    cache: &mut ImportCache,
) -> Result<Vec<IdlInstruction>> {
    let mut instructions: Vec<IdlInstruction> = Vec::new();
    let mut seen = HashSet::new();
    let mut local_enums = Vec::new();

    for (file, item_enum) in enums_with_paths(ctx) {
        if get_derive_attr(&item_enum.attrs, DERIVE_INSTRUCTION_ATTR).is_none()
        {
            continue;
        }

        let import = ShankImport::from_attrs(&item_enum.attrs)
            .map_err(parse_error_into)
            .with_context(|| {
                format!(
                    "While parsing ShankInstruction '{}' in {}",
                    item_enum.ident,
                    file.display()
                )
            })?;
        if let Some(import) = import {
            let renames = type_imports.get(&import.import_from);
            let imported = resolve_imported_instructions(
                import_ctx,
                &import,
                &item_enum.ident.to_string(),
                renames,
                cache,
            )
            .with_context(|| {
                format!(
                    "While importing instructions for '{}' from {}",
                    item_enum.ident,
                    import.import_from
                )
            })?;
            for ix in imported {
                if !seen.insert(ix.name.clone()) {
                    bail!("Duplicate instruction '{}'", ix.name);
                }
                instructions.push(ix);
            }
        } else {
            local_enums.push((file, item_enum));
        }
    }

    // TODO(thlorenz): Should we enforce only one Instruction Enum Arg?
    // TODO(thlorenz): Should unfold that only arg?
    // TODO(thlorenz): Better way to combine those if we don't do the above.
    for (file, item_enum) in local_enums {
        let maybe_ix = Instruction::try_from_item_enum(&item_enum, false)
            .map_err(parse_error_into)
            .with_context(|| {
                format!(
                    "While parsing ShankInstruction '{}' in {}",
                    item_enum.ident,
                    file.display()
                )
            })?;
        if let Some(ix) = maybe_ix {
            let idl_instructions: IdlInstructions = ix.try_into()?;
            for ix in idl_instructions.0 {
                if !seen.insert(ix.name.clone()) {
                    bail!("Duplicate instruction '{}'", ix.name);
                }
                instructions.push(ix);
            }
        }
    }
    Ok(instructions)
}

fn constants(_ctx: &CrateContext) -> Result<Vec<IdlConst>> {
    // TODO(thlorenz): Implement
    let constants: Vec<IdlConst> = Vec::new();
    Ok(constants)
}

fn state(_ctx: &CrateContext) -> Result<Option<IdlState>> {
    // TODO(thlorenz): Implement
    Ok(None)
}

fn types(
    ctx: &CrateContext,
    detect_custom_type: &DetectCustomTypeConfig,
    import_ctx: &ImportContext,
    cache: &mut ImportCache,
) -> Result<(Vec<IdlTypeDefinition>, TypeImportMap)> {
    let mut custom_structs = Vec::new();
    for (file, item) in structs_with_paths(ctx) {
        if !detect_custom_type.are_custom_type_attrs(&item.attrs) {
            continue;
        }
        let strct = CustomStruct::try_from(&item)
            .map_err(parse_error_into)
            .with_context(|| {
                format!(
                    "While parsing ShankType struct '{}' in {}",
                    item.ident,
                    file.display()
                )
            })?;
        custom_structs.push((file, strct));
    }

    let mut custom_enums = Vec::new();
    for (file, item) in enums_with_paths(ctx) {
        if !detect_custom_type.are_custom_type_attrs(&item.attrs) {
            continue;
        }
        let enm = CustomEnum::try_from(&item)
            .map_err(parse_error_into)
            .with_context(|| {
                format!(
                    "While parsing ShankType enum '{}' in {}",
                    item.ident,
                    file.display()
                )
            })?;
        custom_enums.push((file, enm));
    }

    let mut seen = HashSet::new();
    let mut types = Vec::new();
    let mut type_imports: TypeImportMap = HashMap::new();

    for (file, strct) in custom_structs {
        let name = strct.ident.to_string();
        let type_def = if let Some(import) = &strct.import {
            register_type_import(&mut type_imports, import, &name);
            let mut imported = resolve_imported_definition(
                import_ctx,
                import,
                &name,
                type_imports.get(&import.import_from),
                cache,
            )?;
            apply_pod_sentinel_override(&mut imported, &strct)?;
            imported
        } else {
            IdlTypeDefinition::try_from(strct).with_context(|| {
                format!(
                    "While converting ShankType struct '{}' from {} to IDL",
                    name,
                    file.display()
                )
            })?
        };

        if !seen.insert(type_def.name.clone()) {
            bail!("Duplicate type definition '{}'", type_def.name);
        }
        types.push(type_def);
    }

    for (file, enm) in custom_enums {
        let name = enm.ident.to_string();
        let type_def = if let Some(import) = &enm.import {
            register_type_import(&mut type_imports, import, &name);
            resolve_imported_definition(
                import_ctx,
                import,
                &name,
                type_imports.get(&import.import_from),
                cache,
            )?
        } else {
            IdlTypeDefinition::try_from(enm).with_context(|| {
                format!(
                    "While converting ShankType enum '{}' from {} to IDL",
                    name,
                    file.display()
                )
            })?
        };

        if !seen.insert(type_def.name.clone()) {
            bail!("Duplicate type definition '{}'", type_def.name);
        }
        types.push(type_def);
    }

    for (file, item_enum) in enums_with_paths(ctx) {
        if get_derive_attr(&item_enum.attrs, DERIVE_INSTRUCTION_ATTR).is_none() {
            continue;
        }
        let import = ShankImport::from_attrs(&item_enum.attrs)
            .map_err(parse_error_into)
            .with_context(|| {
                format!(
                    "While parsing ShankInstruction '{}' in {}",
                    item_enum.ident,
                    file.display()
                )
            })?;
        let import = match import {
            Some(import) => import,
            None => continue,
        };
        let name = item_enum.ident.to_string();
        register_type_import(&mut type_imports, &import, &name);
        let type_def = resolve_imported_definition(
            import_ctx,
            &import,
            &name,
            type_imports.get(&import.import_from),
            cache,
        )
        .with_context(|| {
            format!(
                "While importing type definition '{}' from {}",
                name,
                import.import_from
            )
        })?;
        if !seen.insert(type_def.name.clone()) {
            bail!("Duplicate type definition '{}'", type_def.name);
        }
        types.push(type_def);
    }

    Ok((types, type_imports))
}

fn register_type_import(
    imports: &mut TypeImportMap,
    import: &ShankImport,
    local_name: &str,
) {
    let external_name = import.rename.as_deref().unwrap_or(local_name);
    imports
        .entry(import.import_from.clone())
        .or_default()
        .insert(external_name.to_string(), local_name.to_string());
}

fn resolve_import_context(filename: &Path) -> Result<ImportContext> {
    let start = filename
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let manifest = Manifest::discover_from_path(start.clone())?;
    let root = manifest
        .as_ref()
        .and_then(|m| m.path().parent().map(Path::to_path_buf))
        .unwrap_or(start);
    Ok(ImportContext { root, manifest })
}

fn resolve_imported_definition(
    import_ctx: &ImportContext,
    import: &ShankImport,
    local_name: &str,
    type_renames: Option<&HashMap<String, String>>,
    cache: &mut ImportCache,
) -> Result<IdlTypeDefinition> {
    let external_name = import.rename.as_deref().unwrap_or(local_name);
    let source = resolve_import_source(import_ctx, &import.import_from)?;
    let mut def = match source {
        ImportSource::IdlPath(path) => {
            let idl = load_import_idl(&path, cache)?;
            find_imported_definition(&idl, external_name)
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Type '{}' not found in imported IDL {}",
                        external_name,
                        path.display()
                    )
                })?
        }
        ImportSource::CrateRoot(root) => {
            let idl_path = crate_idl_path(&root)?;
            let idl = if idl_path.exists() {
                Some(load_import_idl(&idl_path, cache)?)
            } else {
                None
            };
            if let Some(idl) = idl {
                if let Some(def) = find_imported_definition(&idl, external_name) {
                    def.clone()
                } else {
                    find_type_definition_in_crate(&root, external_name)?
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "Type '{}' not found in imported crate {}",
                                external_name,
                                root.display()
                            )
                        })?
                }
            } else {
                find_type_definition_in_crate(&root, external_name)?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Type '{}' not found in imported crate {}",
                            external_name,
                            root.display()
                        )
                    })?
            }
        }
    };
    if let Some(renames) = type_renames {
        rewrite_type_definition(&mut def, renames);
    }
    def.name = local_name.to_string();
    Ok(def)
}

fn resolve_imported_instructions(
    import_ctx: &ImportContext,
    import: &ShankImport,
    local_name: &str,
    type_renames: Option<&HashMap<String, String>>,
    cache: &mut ImportCache,
) -> Result<Vec<IdlInstruction>> {
    let external_name = import.rename.as_deref().unwrap_or(local_name);
    let source = resolve_import_source(import_ctx, &import.import_from)?;
    let mut instructions = match source {
        ImportSource::IdlPath(path) => {
            let idl = load_import_idl(&path, cache)?;
            idl.instructions
        }
        ImportSource::CrateRoot(root) => {
            let idl_path = crate_idl_path(&root)?;
            if idl_path.exists() {
                let idl = load_import_idl(&idl_path, cache)?;
                if !idl.instructions.is_empty() {
                    idl.instructions
                } else {
                    find_instructions_in_crate(
                        &root,
                        external_name,
                    )?
                }
            } else {
                find_instructions_in_crate(&root, external_name)?
            }
        }
    };

    if let Some(renames) = type_renames {
        for ix in &mut instructions {
            for arg in &mut ix.args {
                rewrite_idl_type(&mut arg.ty, renames);
            }
        }
    }

    Ok(instructions)
}

enum ImportSource {
    IdlPath(PathBuf),
    CrateRoot(PathBuf),
}

fn resolve_import_source(
    import_ctx: &ImportContext,
    import_from: &str,
) -> Result<ImportSource> {
    let looks_like_path = import_from.contains('/')
        || import_from.contains('\\')
        || import_from.ends_with(".json");
    if looks_like_path {
        let path = Path::new(import_from);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            import_ctx.root.join(path)
        };
        return Ok(ImportSource::IdlPath(path));
    }

    if let Some(root) = resolve_dependency_root(import_ctx, import_from) {
        return Ok(ImportSource::CrateRoot(root));
    }

    Ok(ImportSource::IdlPath(
        import_ctx.root.join("idl").join(format!("{}.json", import_from)),
    ))
}

fn resolve_dependency_root(
    import_ctx: &ImportContext,
    import_from: &str,
) -> Option<PathBuf> {
    let manifest = import_ctx.manifest.as_ref()?;
    let dep_path = manifest
        .dependency_path(import_from)
        .or_else(|| {
            if import_from.contains('_') {
                manifest.dependency_path(&import_from.replace('_', "-"))
            } else {
                None
            }
        })?;
    let manifest_dir = manifest.path().parent()?;
    Some(manifest_dir.join(dep_path))
}

fn crate_idl_path(crate_root: &Path) -> Result<PathBuf> {
    let manifest_path = crate_root.join("Cargo.toml");
    let manifest = Manifest::from_path(&manifest_path)?;
    let lib_name = manifest.lib_name()?;
    Ok(crate_root.join("idl").join(format!("{}.json", lib_name)))
}

fn resolve_lib_path(crate_root: &Path) -> Result<PathBuf> {
    let manifest_path = crate_root.join("Cargo.toml");
    let manifest = Manifest::from_path(&manifest_path)?;
    let rel_path = manifest
        .lib_rel_path()
        .unwrap_or_else(|| "src/lib.rs".to_string());
    Ok(crate_root.join(rel_path))
}

fn load_import_idl(
    path: &Path,
    cache: &mut ImportCache,
) -> Result<Idl> {
    if let Some(idl) = cache.get(path) {
        return Ok(idl.clone());
    }
    let contents = std::fs::read_to_string(path).with_context(|| {
        format!("Failed to read imported IDL at {}", path.display())
    })?;
    let idl: Idl = serde_json::from_str(&contents).with_context(|| {
        format!("Failed to parse imported IDL at {}", path.display())
    })?;
    cache.insert(path.to_path_buf(), idl.clone());
    Ok(idl)
}

fn find_imported_definition<'a>(
    idl: &'a Idl,
    name: &str,
) -> Option<&'a IdlTypeDefinition> {
    idl.types
        .iter()
        .find(|def| def.name == name)
        .or_else(|| idl.accounts.iter().find(|def| def.name == name))
}

fn find_type_definition_in_crate(
    crate_root: &Path,
    name: &str,
) -> Result<Option<IdlTypeDefinition>> {
    let lib_path = resolve_lib_path(crate_root)?;
    let ctx = CrateContext::parse(lib_path.clone())?;
    let const_lengths = collect_const_lengths(&ctx);
    with_const_lengths(const_lengths, || {
        for (file, item) in structs_with_paths(&ctx) {
            if item.ident != name {
                continue;
            }
            let parsed = ParsedStruct::try_from(&item)
                .map_err(parse_error_into)
                .with_context(|| {
                    format!(
                        "While parsing struct '{}' in {}",
                        name,
                        file.display()
                    )
                })?;
            return Ok(Some(IdlTypeDefinition::try_from(parsed)?));
        }

        for (file, item) in enums_with_paths(&ctx) {
            if item.ident != name {
                continue;
            }
            let parsed = ParsedEnum::try_from(&item)
                .map_err(parse_error_into)
                .with_context(|| {
                    format!(
                        "While parsing enum '{}' in {}",
                        name,
                        file.display()
                    )
                })?;
            let name = parsed.ident.to_string();
            let ty = parsed.try_into()?;
            return Ok(Some(IdlTypeDefinition {
                name,
                ty,
                pod_sentinel: None,
            }));
        }

        Ok(None)
    })
}

fn find_instructions_in_crate(
    crate_root: &Path,
    enum_name: &str,
) -> Result<Vec<IdlInstruction>> {
    let lib_path = resolve_lib_path(crate_root)?;
    let ctx = CrateContext::parse(lib_path.clone())?;
    let item = ctx
        .enums()
        .find(|e| e.ident == enum_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Instruction enum '{}' not found in {}",
                enum_name,
                crate_root.display()
            )
        })?;
    let const_lengths = collect_const_lengths(&ctx);
    with_const_lengths(const_lengths, || {
        let parsed = ParsedEnum::try_from(item)
            .map_err(parse_error_into)
            .with_context(|| {
                format!(
                    "While parsing enum '{}' in {}",
                    enum_name,
                    lib_path.display()
                )
            })?;
        let instruction = Instruction::try_from(&parsed)
            .map_err(parse_error_into)
            .with_context(|| {
                format!(
                    "While building instructions from '{}' in {}",
                    enum_name,
                    lib_path.display()
                )
            })?;
        let idl_instructions: IdlInstructions = instruction.try_into()?;
        Ok(idl_instructions.0)
    })
}

fn rewrite_idl_type(
    ty: &mut IdlType,
    renames: &HashMap<String, String>,
) {
    match ty {
        IdlType::Defined(name) => {
            if let Some(new_name) = renames.get(name) {
                *name = new_name.clone();
            }
        }
        IdlType::Vec(inner)
        | IdlType::Option(inner)
        | IdlType::HashSet(inner)
        | IdlType::BTreeSet(inner) => rewrite_idl_type(inner, renames),
        IdlType::Array(inner, _) => rewrite_idl_type(inner, renames),
        IdlType::HashMap(key, val)
        | IdlType::BTreeMap(key, val) => {
            rewrite_idl_type(key, renames);
            rewrite_idl_type(val, renames);
        }
        IdlType::Tuple(types) => {
            for t in types {
                rewrite_idl_type(t, renames);
            }
        }
        IdlType::FixedSizeOption { inner, .. } => {
            rewrite_idl_type(inner, renames);
        }
        _ => {}
    }
}

fn rewrite_type_definition(
    type_def: &mut IdlTypeDefinition,
    renames: &HashMap<String, String>,
) {
    match &mut type_def.ty {
        crate::idl_type_definition::IdlTypeDefinitionTy::Struct { fields } => {
            for field in fields {
                rewrite_idl_type(&mut field.ty, renames);
            }
        }
        crate::idl_type_definition::IdlTypeDefinitionTy::Enum { variants } => {
            for variant in variants {
                if let Some(fields) = &mut variant.fields {
                    match fields {
                        crate::idl_variant::EnumFields::Named(named_fields) => {
                            for field in named_fields {
                                rewrite_idl_type(&mut field.ty, renames);
                            }
                        }
                        crate::idl_variant::EnumFields::Tuple(tuple_types) => {
                            for ty in tuple_types {
                                rewrite_idl_type(ty, renames);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn apply_pod_sentinel_override(
    type_def: &mut IdlTypeDefinition,
    strct: &CustomStruct,
) -> Result<()> {
    let pod_sentinel = strct
        .0
        .struct_attrs
        .items_ref()
        .iter()
        .find_map(|attr| match attr {
            StructAttr::PodSentinel(sentinel) => Some(sentinel.clone()),
            _ => None,
        });

    if let Some(local) = pod_sentinel {
        if let Some(existing) = &type_def.pod_sentinel {
            if existing != &local {
                bail!(
                    "Proxy type '{}' defines pod_sentinel {:?} but imported type has {:?}",
                    type_def.name,
                    local,
                    existing
                );
            }
        }
        type_def.pod_sentinel = Some(local);
    }

    Ok(())
}

fn collect_const_lengths(ctx: &CrateContext) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    for item in ctx.consts() {
        if let Expr::Lit(ExprLit {
            lit: Lit::Int(int_lit),
            ..
        }) = item.expr.as_ref()
        {
            if let Ok(value) = int_lit.base10_parse::<usize>() {
                map.insert(item.ident.to_string(), value);
            }
        }
    }
    map
}

fn metadata(
    ctx: &CrateContext,
    require_program_address: bool,
    program_address_override: Option<&String>,
) -> Result<IdlMetadata> {
    let macros: Vec<_> = ctx.macros().cloned().collect();
    let address = if let Some(program_address) = program_address_override {
        Ok(Some(program_address.clone()))
    } else {
        match ProgramId::try_from(&macros[..]) {
            Ok(ProgramId { id }) => Ok(Some(id)),
            Err(err) if require_program_address => Err(err),
            Err(_) => Ok(None),
        }
    }?;
    Ok(IdlMetadata {
        origin: "shank".to_string(),
        address,
    })
}

fn events(_ctx: &CrateContext) -> Result<Option<Vec<IdlEvent>>> {
    // TODO(thlorenz): Implement
    Ok(None)
}

fn errors(ctx: &CrateContext) -> Result<Option<Vec<IdlErrorCode>>> {
    let mut program_errors = Vec::new();
    for (file, item_enum) in enums_with_paths(ctx) {
        if get_derive_attr(&item_enum.attrs, DERIVE_THIS_ERROR_ATTR).is_none()
        {
            continue;
        }
        let parsed = ParsedEnum::try_from(&item_enum)
            .map_err(parse_error_into)
            .with_context(|| {
                format!(
                    "While parsing error enum '{}' in {}",
                    item_enum.ident,
                    file.display()
                )
            })?;
        let errors = ProgramErrors::try_from(&parsed)
            .map_err(parse_error_into)
            .with_context(|| {
                format!(
                    "While parsing #[error] attributes for enum '{}' in {}",
                    item_enum.ident,
                    file.display()
                )
            })?;
        program_errors.extend(errors.0);
    }
    if program_errors.is_empty() {
        Ok(None)
    } else {
        let error_codes = program_errors
            .into_iter()
            .map(IdlErrorCode::from)
            .collect::<Vec<IdlErrorCode>>();
        Ok(Some(error_codes))
    }
}

/// Walks all IdlType instances in the IDL, calling the provided closure for each type
/// including nested types within containers (Vec, Option, HashMap, etc.)
fn walk_idl_types<F>(idl: &mut Idl, mut f: F)
where
    F: FnMut(&mut IdlType),
{
    fn walk_type<F>(ty: &mut IdlType, f: &mut F)
    where
        F: FnMut(&mut IdlType),
    {
        // Call the closure on this type
        f(ty);

        // Recursively walk nested types
        match ty {
            IdlType::Vec(inner)
            | IdlType::Option(inner)
            | IdlType::HashSet(inner)
            | IdlType::BTreeSet(inner) => {
                walk_type(inner, f);
            }
            IdlType::Array(inner, _) => {
                walk_type(inner, f);
            }
            IdlType::HashMap(key, val) | IdlType::BTreeMap(key, val) => {
                walk_type(key, f);
                walk_type(val, f);
            }
            IdlType::Tuple(types) => {
                for t in types {
                    walk_type(t, f);
                }
            }
            IdlType::FixedSizeOption { inner, .. } => {
                walk_type(inner, f);
            }
            _ => {}
        }
    }

    // Walk all account fields
    for account in &mut idl.accounts {
        match &mut account.ty {
            crate::idl_type_definition::IdlTypeDefinitionTy::Struct { fields } => {
                for field in fields {
                    walk_type(&mut field.ty, &mut f);
                }
            }
            crate::idl_type_definition::IdlTypeDefinitionTy::Enum { variants } => {
                for variant in variants {
                    if let Some(fields) = &mut variant.fields {
                        match fields {
                            crate::idl_variant::EnumFields::Named(named_fields) => {
                                for field in named_fields {
                                    walk_type(&mut field.ty, &mut f);
                                }
                            }
                            crate::idl_variant::EnumFields::Tuple(tuple_types) => {
                                for ty in tuple_types {
                                    walk_type(ty, &mut f);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Walk all custom type fields
    for type_def in &mut idl.types {
        match &mut type_def.ty {
            crate::idl_type_definition::IdlTypeDefinitionTy::Struct { fields } => {
                for field in fields {
                    walk_type(&mut field.ty, &mut f);
                }
            }
            crate::idl_type_definition::IdlTypeDefinitionTy::Enum { variants } => {
                for variant in variants {
                    if let Some(fields) = &mut variant.fields {
                        match fields {
                            crate::idl_variant::EnumFields::Named(named_fields) => {
                                for field in named_fields {
                                    walk_type(&mut field.ty, &mut f);
                                }
                            }
                            crate::idl_variant::EnumFields::Tuple(tuple_types) => {
                                for ty in tuple_types {
                                    walk_type(ty, &mut f);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Walk all instruction arguments
    for instruction in &mut idl.instructions {
        for arg in &mut instruction.args {
            walk_type(&mut arg.ty, &mut f);
        }
    }
}

/// Walks all IdlType instances in the IDL (immutable version)
fn walk_idl_types_ref<F>(idl: &Idl, mut f: F)
where
    F: FnMut(&IdlType),
{
    fn walk_type<F>(ty: &IdlType, f: &mut F)
    where
        F: FnMut(&IdlType),
    {
        // Call the closure on this type
        f(ty);

        // Recursively walk nested types
        match ty {
            IdlType::Vec(inner)
            | IdlType::Option(inner)
            | IdlType::HashSet(inner)
            | IdlType::BTreeSet(inner) => {
                walk_type(inner, f);
            }
            IdlType::Array(inner, _) => {
                walk_type(inner, f);
            }
            IdlType::HashMap(key, val) | IdlType::BTreeMap(key, val) => {
                walk_type(key, f);
                walk_type(val, f);
            }
            IdlType::Tuple(types) => {
                for t in types {
                    walk_type(t, f);
                }
            }
            IdlType::FixedSizeOption { inner, .. } => {
                walk_type(inner, f);
            }
            _ => {}
        }
    }

    // Walk all account fields
    for account in &idl.accounts {
        match &account.ty {
            crate::idl_type_definition::IdlTypeDefinitionTy::Struct { fields } => {
                for field in fields {
                    walk_type(&field.ty, &mut f);
                }
            }
            crate::idl_type_definition::IdlTypeDefinitionTy::Enum { variants } => {
                for variant in variants {
                    if let Some(fields) = &variant.fields {
                        match fields {
                            crate::idl_variant::EnumFields::Named(named_fields) => {
                                for field in named_fields {
                                    walk_type(&field.ty, &mut f);
                                }
                            }
                            crate::idl_variant::EnumFields::Tuple(tuple_types) => {
                                for ty in tuple_types {
                                    walk_type(ty, &mut f);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Walk all custom type fields
    for type_def in &idl.types {
        match &type_def.ty {
            crate::idl_type_definition::IdlTypeDefinitionTy::Struct { fields } => {
                for field in fields {
                    walk_type(&field.ty, &mut f);
                }
            }
            crate::idl_type_definition::IdlTypeDefinitionTy::Enum { variants } => {
                for variant in variants {
                    if let Some(fields) = &variant.fields {
                        match fields {
                            crate::idl_variant::EnumFields::Named(named_fields) => {
                                for field in named_fields {
                                    walk_type(&field.ty, &mut f);
                                }
                            }
                            crate::idl_variant::EnumFields::Tuple(tuple_types) => {
                                for ty in tuple_types {
                                    walk_type(ty, &mut f);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Walk all instruction arguments
    for instruction in &idl.instructions {
        for arg in &instruction.args {
            walk_type(&arg.ty, &mut f);
        }
    }
}

fn populate_pod_option_sentinels(idl: &mut Idl) -> Result<()> {
    use std::collections::HashMap;

    // Build a map of custom type names to their podSentinel (owned data)
    let type_sentinels: HashMap<String, Vec<u8>> = idl
        .types
        .iter()
        .filter_map(|type_def| {
            type_def.pod_sentinel.as_ref().map(|sentinel| {
                (type_def.name.clone(), sentinel.clone())
            })
        })
        .collect();

    // Walk all IdlType instances and populate sentinels for PodOption<CustomType>
    walk_idl_types(idl, |ty| {
        if let IdlType::FixedSizeOption { inner, sentinel } = ty {
            if let IdlType::Defined(type_name) = inner.as_ref() {
                // If sentinel is not already set, populate it from the type definition
                if sentinel.is_none() {
                    if let Some(type_sentinel) = type_sentinels.get(type_name) {
                        *sentinel = Some(type_sentinel.clone());
                    }
                }
            }
        }
    });

    Ok(())
}

fn validate_pod_option_sentinels(idl: &Idl) -> Result<()> {
    let mut errors = Vec::new();

    // Walk all IdlType instances and check for missing sentinels
    walk_idl_types_ref(idl, |ty| {
        if let IdlType::FixedSizeOption { inner, sentinel } = ty {
            if let IdlType::Defined(type_name) = inner.as_ref() {
                // This is PodOption<CustomType>
                // After population, sentinel should be present
                if sentinel.is_none() {
                    errors.push(format!(
                        "Type '{}' is used in PodOption but does not define #[pod_sentinel(...)]. \
                         Custom types used with PodOption must specify a sentinel value.",
                        type_name
                    ));
                }
            }
        }
    });

    if !errors.is_empty() {
        anyhow::bail!("PodOption validation errors:\n  - {}", errors.join("\n  - "));
    }

    Ok(())
}
