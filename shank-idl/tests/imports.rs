use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use shank_idl::{idl::Idl, parse_file, ParseIdlConfig};

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

fn as_value(idl: &Idl) -> Value {
    serde_json::to_value(idl).expect("Should serialize IDL")
}

fn find_named<'a>(items: &'a [Value], name: &str) -> &'a Value {
    items
        .iter()
        .find(|item| item["name"] == name)
        .unwrap_or_else(|| panic!("Missing item '{}'", name))
}

fn fields_for(def: &Value) -> &[Value] {
    def["type"]["fields"]
        .as_array()
        .expect("Expected fields array")
}

fn assert_field_type(fields: &[Value], name: &str, expected: Value) {
    let field = fields
        .iter()
        .find(|field| field["name"] == name)
        .unwrap_or_else(|| panic!("Missing field '{}'", name));
    assert_eq!(field["type"], expected);
}

#[test]
fn import_case1_simple_proxy() {
    let idl = parse_case("case1");
    let value = as_value(&idl);
    let types = value["types"].as_array().expect("types array");

    let proxy = find_named(types, "RemoteRouterConfigProxy");
    let fields = fields_for(proxy);
    assert_field_type(fields, "domain", json!("u32"));
    assert_field_type(
        fields,
        "router",
        json!({"option": {"array": ["u8", 32]}}),
    );

    let local = find_named(types, "LocalConfig");
    let local_fields = fields_for(local);
    assert_field_type(
        local_fields,
        "remote",
        json!({"defined": "RemoteRouterConfigProxy"}),
    );
}

#[test]
fn import_case2_account_proxy() {
    let idl = parse_case("case2");
    let value = as_value(&idl);
    let types = value["types"].as_array().expect("types array");
    let accounts = value["accounts"].as_array().expect("accounts array");

    let proxy = find_named(types, "MerkleTreeProxy");
    let fields = fields_for(proxy);
    assert_field_type(
        fields,
        "branch",
        json!({"array": [{"array": ["u8", 32]}, 32]}),
    );
    assert_field_type(fields, "count", json!("u64"));

    let outbox = find_named(accounts, "Outbox");
    let outbox_fields = fields_for(outbox);
    assert_field_type(
        outbox_fields,
        "tree",
        json!({"defined": "MerkleTreeProxy"}),
    );
}

#[test]
fn import_case3_instruction_proxy() {
    let idl = parse_case("case3");
    let value = as_value(&idl);
    let types = value["types"].as_array().expect("types array");
    let instructions = value["instructions"]
        .as_array()
        .expect("instructions array");

    let init_proxy = find_named(types, "InitProxy");
    let init_fields = fields_for(init_proxy);
    assert_field_type(init_fields, "mailbox", json!({"array": ["u8", 32]}));
    assert_field_type(init_fields, "decimals", json!("u8"));

    let transfer_proxy = find_named(types, "TransferRemoteProxy");
    let transfer_fields = fields_for(transfer_proxy);
    assert_field_type(transfer_fields, "destinationDomain", json!("u32"));
    assert_field_type(
        transfer_fields,
        "recipient",
        json!({"array": ["u8", 32]}),
    );

    let instruction_proxy = find_named(types, "TokenInstructionProxy");
    let variants = instruction_proxy["type"]["variants"]
        .as_array()
        .expect("variants array");
    let init_variant = find_named(variants, "Init");
    let transfer_variant = find_named(variants, "TransferRemote");
    assert_eq!(
        init_variant["fields"],
        json!([{"defined": "InitProxy"}])
    );
    assert_eq!(
        transfer_variant["fields"],
        json!([{"defined": "TransferRemoteProxy"}])
    );

    let init_ix = find_named(instructions, "Init");
    assert_eq!(init_ix["args"][0]["type"], json!({"defined": "InitProxy"}));

    let transfer_ix = find_named(instructions, "TransferRemote");
    assert_eq!(
        transfer_ix["args"][0]["type"],
        json!({"defined": "TransferRemoteProxy"})
    );

    let base_ix = find_named(instructions, "Base");
    assert_eq!(
        base_ix["args"][0]["type"],
        json!({"defined": "TokenInstructionProxy"})
    );
}
