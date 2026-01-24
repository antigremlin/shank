use std::path::{Path, PathBuf};

use shank_idl::{
    idl::{Idl, IdlInstruction},
    idl_field::IdlField,
    idl_type::IdlType,
    idl_type_definition::IdlTypeDefinition,
    idl_type_definition::IdlTypeDefinitionTy,
    idl_variant::EnumFields,
    parse_file, ParseIdlConfig,
};

fn fixtures_dir() -> PathBuf {
    let root_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    root_dir.join("tests").join("fixtures").join("imports")
}

fn parse_case(case: &str) -> Idl {
    let file = fixtures_dir()
        .join(case)
        .join("importing-program")
        .join("src")
        .join("lib.rs");
    parse_file(file, &ParseIdlConfig::optional_program_address())
        .expect("Parsing should not fail")
        .expect("File contains IDL")
}

fn find_type<'a>(
    types: &'a [IdlTypeDefinition],
    name: &str,
) -> &'a IdlTypeDefinition {
    types
        .iter()
        .find(|def| def.name == name)
        .unwrap_or_else(|| panic!("Missing type '{}'", name))
}

fn find_instruction<'a>(
    instructions: &'a [IdlInstruction],
    name: &str,
) -> &'a IdlInstruction {
    instructions
        .iter()
        .find(|ix| ix.name == name)
        .unwrap_or_else(|| panic!("Missing instruction '{}'", name))
}

fn find_struct_fields<'a>(
    def: &'a IdlTypeDefinition,
) -> &'a [IdlField] {
    match &def.ty {
        IdlTypeDefinitionTy::Struct { fields } => fields,
        _ => panic!("Expected struct type definition for {}", def.name),
    }
}

fn find_enum_variants<'a>(
    def: &'a IdlTypeDefinition,
) -> &'a [shank_idl::idl_variant::IdlEnumVariant] {
    match &def.ty {
        IdlTypeDefinitionTy::Enum { variants } => variants,
        _ => panic!("Expected enum type definition for {}", def.name),
    }
}

fn assert_field_type(fields: &[IdlField], name: &str, expected: &IdlType) {
    let field = fields
        .iter()
        .find(|field| field.name == name)
        .unwrap_or_else(|| panic!("Missing field '{}'", name));
    assert_eq!(&field.ty, expected);
}

#[test]
fn import_case1_simple_proxy() {
    let idl = parse_case("case1");

    let proxy = find_type(&idl.types, "RemoteRouterConfigProxy");
    let fields = find_struct_fields(proxy);
    assert_field_type(fields, "domain", &IdlType::U32);
    assert_field_type(
        fields,
        "router",
        &IdlType::Option(Box::new(IdlType::Array(
            Box::new(IdlType::U8),
            32,
        ))),
    );

    let local = find_type(&idl.types, "LocalConfig");
    let local_fields = find_struct_fields(local);
    assert_field_type(
        local_fields,
        "remote",
        &IdlType::Defined("RemoteRouterConfigProxy".to_string()),
    );
}

#[test]
fn import_case2_account_proxy() {
    let idl = parse_case("case2");

    let proxy = find_type(&idl.types, "MerkleTreeProxy");
    let fields = find_struct_fields(proxy);
    assert_field_type(
        fields,
        "branch",
        &IdlType::Array(
            Box::new(IdlType::Array(Box::new(IdlType::U8), 32)),
            32,
        ),
    );
    assert_field_type(fields, "count", &IdlType::U64);

    let outbox = find_type(&idl.accounts, "Outbox");
    let outbox_fields = find_struct_fields(outbox);
    assert_field_type(
        outbox_fields,
        "tree",
        &IdlType::Defined("MerkleTreeProxy".to_string()),
    );
}

#[test]
fn import_case3_instruction_proxy() {
    let idl = parse_case("case3");

    let init_proxy = find_type(&idl.types, "InitProxy");
    let init_fields = find_struct_fields(init_proxy);
    assert_field_type(
        init_fields,
        "mailbox",
        &IdlType::Array(Box::new(IdlType::U8), 32),
    );
    assert_field_type(init_fields, "decimals", &IdlType::U8);

    let transfer_proxy = find_type(&idl.types, "TransferRemoteProxy");
    let transfer_fields = find_struct_fields(transfer_proxy);
    assert_field_type(transfer_fields, "destination_domain", &IdlType::U32);
    assert_field_type(
        transfer_fields,
        "recipient",
        &IdlType::Array(Box::new(IdlType::U8), 32),
    );

    let instruction_proxy = find_type(&idl.types, "TokenInstructionProxy");
    let variants = find_enum_variants(instruction_proxy);
    let init_variant = variants
        .iter()
        .find(|variant| variant.name == "Init")
        .expect("Missing Init variant on TokenInstructionProxy");
    let transfer_variant = variants
        .iter()
        .find(|variant| variant.name == "TransferRemote")
        .expect("Missing TransferRemote variant on TokenInstructionProxy");

    match &init_variant.fields {
        Some(EnumFields::Tuple(types)) => {
            assert_eq!(
                types,
                &vec![IdlType::Defined("InitProxy".to_string())]
            );
        }
        _ => panic!("Expected tuple fields for Init variant"),
    }

    match &transfer_variant.fields {
        Some(EnumFields::Tuple(types)) => {
            assert_eq!(
                types,
                &vec![IdlType::Defined("TransferRemoteProxy".to_string())]
            );
        }
        _ => panic!("Expected tuple fields for TransferRemote variant"),
    }

    let init_ix = find_instruction(&idl.instructions, "Init");
    assert_eq!(init_ix.args.len(), 1);
    assert_eq!(
        init_ix.args[0].ty,
        IdlType::Defined("InitProxy".to_string())
    );

    let transfer_ix = find_instruction(&idl.instructions, "TransferRemote");
    assert_eq!(transfer_ix.args.len(), 1);
    assert_eq!(
        transfer_ix.args[0].ty,
        IdlType::Defined("TransferRemoteProxy".to_string())
    );

    let base_ix = find_instruction(&idl.instructions, "Base");
    assert_eq!(base_ix.args.len(), 1);
    assert_eq!(
        base_ix.args[0].ty,
        IdlType::Defined("TokenInstructionProxy".to_string())
    );
}
