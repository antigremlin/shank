# TASK: Support Cross-Library Type Proxies in Shank IDL Generation

## Context

When importing instruction enums that reference types from different libraries, Shank currently only applies type proxy renames from the same import source as the instruction enum. This prevents proper type resolution in consuming programs.

### Example Scenario
```rust
// In token-native program
#[shank(import_from = "token_lib")]           // Source A
pub enum TokenInstructionProxy {}

#[shank(import_from = "connection_client")]   // Source B (different!)
pub struct RemoteRouterConfigProxy;

// Problem: Shank only looks at Source A proxies when rewriting
// instructions imported from Source A, ignoring Source B proxies
```

### Current Behavior
- Instructions reference original type name: `RemoteRouterConfig`
- Proxy is defined with different name: `RemoteRouterConfigProxy`
- Mismatch in IDL: type reference doesn't match definition

## Goal

Enable Shank to resolve type proxies across ALL import sources when processing imported instruction enums, not just from the same source.

## Implementation Design

### Changes Required

**File: `shank-idl/src/file.rs`**

#### 1. Modify `resolve_imported_instructions()` (lines ~509-550)

**Current signature:**
```rust
fn resolve_imported_instructions(
    import_ctx: &ImportContext,
    import: &ShankImport,
    local_name: &str,
    type_renames: Option<&HashMap<String, String>>,  // Only one source
    cache: &mut ImportCache,
) -> Result<Vec<IdlInstruction>>
```

**New signature:**
```rust
fn resolve_imported_instructions(
    import_ctx: &ImportContext,
    import: &ShankImport,
    local_name: &str,
    all_type_imports: &TypeImportMap,  // All sources
    cache: &mut ImportCache,
) -> Result<Vec<IdlInstruction>>
```

**Update the rewriting loop:**
```rust
// Current code applies renames only from matching source
if let Some(renames) = type_renames {
    for ix in &mut instructions {
        for arg in &mut ix.args {
            rewrite_idl_type(&mut arg.ty, renames);
        }
    }
}

// New code passes all sources to rewrite function
for ix in &mut instructions {
    for arg in &mut ix.args {
        rewrite_idl_type(&mut arg.ty, all_type_imports, Some(&import.import_from));
    }
}
```

#### 2. Update call site in `instructions()` (lines ~192-268)

**Current:**
```rust
let type_imports = import_ctx.type_imports.get(&import.import_from);
resolve_imported_instructions(
    &import_ctx,
    &import,
    local_name,
    type_imports,
    &mut import_cache
)?
```

**New:**
```rust
resolve_imported_instructions(
    &import_ctx,
    &import,
    local_name,
    &import_ctx.type_imports,  // Pass entire map
    &mut import_cache
)?
```

#### 3. Modify `rewrite_idl_type()` (lines ~735-765)

**Current signature:**
```rust
fn rewrite_idl_type(
    ty: &mut IdlType,
    renames: &HashMap<String, String>,  // Single source
)
```

**New signature:**
```rust
fn rewrite_idl_type(
    ty: &mut IdlType,
    all_renames: &TypeImportMap,  // All sources
    priority_source: Option<&str>,  // Prefer matches from this source
)
```

**New implementation:**
```rust
fn rewrite_idl_type(
    ty: &mut IdlType,
    all_renames: &TypeImportMap,
    priority_source: Option<&str>,
) {
    match ty {
        IdlType::Defined(name) => {
            // Try priority source first if specified (preserves existing behavior)
            if let Some(source) = priority_source {
                if let Some(source_renames) = all_renames.get(source) {
                    if let Some(new_name) = source_renames.get(name) {
                        *name = new_name.clone();
                        return;
                    }
                }
            }

            // Search all other sources
            let mut found_matches = Vec::new();
            for (source, renames) in all_renames.iter() {
                if priority_source.map_or(true, |ps| ps != source) {
                    if let Some(new_name) = renames.get(name) {
                        found_matches.push((source.clone(), new_name.clone()));
                    }
                }
            }

            // Handle results
            match found_matches.len() {
                0 => {}, // No match found, keep original name
                1 => {
                    *name = found_matches[0].1.clone();
                }
                _ => {
                    // Multiple matches - could error or use first match
                    // For now, use first match with a warning
                    eprintln!("Warning: Ambiguous type rename for '{}' found in sources: {:?}",
                             name, found_matches.iter().map(|(s, _)| s).collect::<Vec<_>>());
                    *name = found_matches[0].1.clone();
                }
            }
        }

        // Recursively handle nested types
        IdlType::Option(inner) => {
            rewrite_idl_type(inner, all_renames, priority_source);
        }
        IdlType::Vec(inner) => {
            rewrite_idl_type(inner, all_renames, priority_source);
        }
        IdlType::Array(inner, _) => {
            rewrite_idl_type(inner, all_renames, priority_source);
        }
        IdlType::Tuple(inners) => {
            for inner in inners {
                rewrite_idl_type(inner, all_renames, priority_source);
            }
        }
        IdlType::HashMap(key, value) => {
            rewrite_idl_type(key, all_renames, priority_source);
            rewrite_idl_type(value, all_renames, priority_source);
        }

        // Primitives don't need rewriting
        IdlType::Bool | IdlType::U8 | IdlType::I8 | IdlType::U16 | IdlType::I16 |
        IdlType::U32 | IdlType::I32 | IdlType::F32 | IdlType::U64 | IdlType::I64 |
        IdlType::F64 | IdlType::U128 | IdlType::I128 | IdlType::U256 | IdlType::I256 |
        IdlType::Bytes | IdlType::String | IdlType::PublicKey => {}
    }
}
```

### Testing

**Add test case: `tests/fixtures/imports/case4/`**

**Structure:**
```
case4/
├── lib-a/         # Defines shared types
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs
├── lib-b/         # Defines instruction enum using types from lib-a
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs
└── program/       # Imports instructions from lib-b, creates proxies for lib-a types
    ├── Cargo.toml
    └── src/
        └── lib.rs
```

**File: `case4/lib-a/src/lib.rs`**
```rust
use shank::ShankType;

#[derive(ShankType)]
pub struct RemoteRouterConfig {
    pub domain: u32,
    pub router: Option<[u8; 32]>,
}

#[derive(ShankType)]
pub struct GasRouterConfig {
    pub domain: u32,
    pub gas: Option<u64>,
}
```

**File: `case4/lib-b/src/lib.rs`**
```rust
use shank::ShankInstruction;

#[derive(ShankInstruction)]
pub enum Instruction {
    EnrollRemoteRouter(lib_a::RemoteRouterConfig),
    SetGasConfig(lib_a::GasRouterConfig),
}
```

**File: `case4/program/src/lib.rs`**
```rust
use shank::{ShankInstruction, ShankType};

#[derive(ShankInstruction)]
#[shank(import_from = "lib-b", rename = "Instruction")]
pub enum InstructionProxy {}

#[derive(ShankType)]
#[shank(import_from = "lib-a", rename = "RemoteRouterConfig")]
pub struct RemoteRouterConfigProxy;

#[derive(ShankType)]
#[shank(import_from = "lib-a", rename = "GasRouterConfig")]
pub struct GasRouterConfigProxy;
```

**Expected IDL output: `case4/program/idl/program.json`**
```json
{
  "version": "0.1.0",
  "name": "program",
  "instructions": [
    {
      "name": "EnrollRemoteRouter",
      "args": [{
        "name": "remoteRouterConfig",
        "type": {"defined": "RemoteRouterConfigProxy"}
      }]
    },
    {
      "name": "SetGasConfig",
      "args": [{
        "name": "gasRouterConfig",
        "type": {"defined": "GasRouterConfigProxy"}
      }]
    }
  ],
  "types": [
    {
      "name": "RemoteRouterConfigProxy",
      "type": {
        "kind": "struct",
        "fields": [
          {"name": "domain", "type": "u32"},
          {"name": "router", "type": {"option": {"array": ["u8", 32]}}}
        ]
      }
    },
    {
      "name": "GasRouterConfigProxy",
      "type": {
        "kind": "struct",
        "fields": [
          {"name": "domain", "type": "u32"},
          {"name": "gas", "type": {"option": "u64"}}
        ]
      }
    }
  ]
}
```

**Add test in `shank-idl/tests/integration_tests.rs`**
```rust
#[test]
fn test_case4_cross_library_type_proxies() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let fixtures = Path::new(manifest_dir).join("tests/fixtures/imports/case4");

    // Test that program IDL correctly uses proxy names
    let program_idl = fixtures.join("program/idl/program.json");
    let content = fs::read_to_string(program_idl).unwrap();
    let idl: serde_json::Value = serde_json::from_str(&content).unwrap();

    // Check instruction args use proxy types
    let instructions = &idl["instructions"];
    assert_eq!(
        instructions[0]["args"][0]["type"]["defined"],
        "RemoteRouterConfigProxy"
    );
    assert_eq!(
        instructions[1]["args"][0]["type"]["defined"],
        "GasRouterConfigProxy"
    );

    // Check proxy types are defined
    let types = &idl["types"];
    let type_names: Vec<&str> = types
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(type_names.contains(&"RemoteRouterConfigProxy"));
    assert!(type_names.contains(&"GasRouterConfigProxy"));
}
```

## Implementation Checklist

- [ ] Modify `resolve_imported_instructions()` signature and implementation
- [ ] Update call site in `instructions()` function
- [ ] Modify `rewrite_idl_type()` signature and implementation
- [ ] Add test case structure under `tests/fixtures/imports/case4/`
- [ ] Create test Cargo workspaces for lib-a, lib-b, and program
- [ ] Add integration test in `tests/integration_tests.rs`
- [ ] Run all existing tests to ensure backward compatibility
- [ ] Document the change in CHANGELOG

## Design Decisions & Considerations

### 1. Priority Source Strategy
Using `priority_source` parameter ensures backward compatibility:
- First tries proxies from the same import source (existing behavior)
- Then searches other sources (new capability)
- Minimizes risk of breaking existing codebases

### 2. Ambiguity Handling
When multiple sources define proxies for the same type:
- **Current implementation**: Use first match with warning
- **Alternative**: Error on ambiguous matches (stricter, may break existing code)
- **Consideration**: In practice, ambiguity is rare and usually indicates a configuration error

### 3. Performance Impact
- Searching all sources vs single source: negligible
- Type import maps are small (typically < 100 entries)
- Lookup is O(n) where n = number of import sources (usually < 10)

### 4. Backward Compatibility
- Priority source ensures existing behavior is preserved
- New functionality only activates when cross-library proxies are present
- All existing tests should pass unchanged

## Files to Modify

- `shank-idl/src/file.rs` - Core changes (3 functions)
- `shank-idl/tests/fixtures/imports/case4/` - New test case
- `shank-idl/tests/integration_tests.rs` - Add test
- `CHANGELOG.md` - Document feature

## Expected Benefits

1. **Cleaner IDLs**: Consuming programs can define proper type proxies for cross-library dependencies
2. **Better modularity**: Libraries don't need to re-export types just for IDL generation
3. **More flexible**: Supports complex dependency graphs without workarounds

## Alternative Considered

**Re-export workaround**: Make libraries re-export types from dependencies
- Simpler to implement (no Shank changes)
- Pollutes library API with unrelated types
- Doesn't scale with many cross-library dependencies
- This is the current temporary solution in hl-solana-idl repo
