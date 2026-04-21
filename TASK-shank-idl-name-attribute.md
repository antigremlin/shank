# Shank Enhancement: `idl_name` Attribute for Type Proxies

## Problem Statement

When using Shank's type import feature with proxies, the proxy struct name becomes the type name in the generated IDL. This leads to suffixes like `*Proxy` appearing in IDL files:

```json
{
  "name": "EnrollRemoteRouter",
  "args": [{
    "type": { "defined": "RemoteRouterConfigProxy" }
  }]
}
```

While the `*Proxy` suffix is useful in Rust code (clearly indicating IDL-only constructs), it's less desirable in the final IDL consumed by external tooling.

## Why Other Approaches Don't Work

### Option 1: Module Namespacing
Putting proxies in a submodule and using qualified paths in `#[idl_type]`:

```rust
mod idl {
    pub struct RemoteRouterConfig;  // Proxy
}

#[idl_type("idl::RemoteRouterConfig")]
EnrollRemoteRouter(...);
```

**Status:** Not supported - Shank doesn't support module paths in `#[idl_type]`

### Option 2: Remove Proxy Suffix
Name the proxy without suffix and use fully qualified paths for real types:

```rust
pub struct RemoteRouterConfig;  // Proxy

EnrollRemoteRouter(hyperlane_sealevel_connection_client::router::RemoteRouterConfig);
```

**Status:** Doesn't work - Rust name resolution prefers local types over fully qualified paths, causing the compiler to use the proxy (which lacks Debug, PartialEq, etc.) instead of the real type.

## Proposed Solution: `idl_name` Attribute

Add an optional `idl_name` parameter to the `#[shank(...)]` attribute that specifies the type name to use in the IDL, separate from the Rust struct name.

### Syntax

```rust
#[derive(BorshDeserialize, BorshSerialize, ShankType)]
#[shank(
    import_from = "hyperlane_sealevel_connection_client",
    rename = "RemoteRouterConfig",
    idl_name = "RemoteRouterConfig"  // NEW: Override IDL type name
)]
pub struct RemoteRouterConfigProxy;  // Rust struct name (clear intent)
```

### Behavior

1. **Rust code:** Uses `RemoteRouterConfigProxy` (clear, no collisions)
2. **Generated IDL:** Uses `RemoteRouterConfig` (clean, no suffix)
3. **Backwards compatible:** If `idl_name` is omitted, use struct name (current behavior)

## Implementation Plan

### 1. Update ShankType Attribute Parsing

**File:** `shank-macro-impl/src/parsers/attributes.rs`

Add `idl_name` field to the attribute structure:

```rust
pub struct ShankTypeAttribute {
    pub import_from: Option<String>,
    pub rename: Option<String>,
    pub idl_name: Option<String>,  // NEW
}
```

Parse the new parameter:

```rust
fn parse_shank_attribute(attr: &Attribute) -> Result<ShankTypeAttribute> {
    // ... existing parsing ...

    if let Some(name_val) = find_meta_name_value(&nested, "idl_name") {
        if let Lit::Str(lit) = &name_val.lit {
            result.idl_name = Some(lit.value());
        }
    }

    // ... rest of parsing ...
}
```

### 2. Use `idl_name` in IDL Generation

**File:** `shank-idl/src/idl.rs`

When generating the IDL type definition, use `idl_name` if present, otherwise fall back to struct name:

```rust
fn generate_type_definition(type_info: &TypeInfo) -> IdlTypeDef {
    let name = type_info.idl_name
        .as_ref()
        .unwrap_or(&type_info.struct_name)
        .clone();

    IdlTypeDef {
        name,
        ty: type_info.ty.clone(),
    }
}
```

### 3. Update Type Reference Resolution

**File:** `shank-idl/src/file.rs`

When resolving type references in instructions, use `idl_name` for lookups:

```rust
fn resolve_type_reference(
    type_ref: &str,
    import_ctx: &ImportContext,
) -> Result<String> {
    // Check if this is a proxy type with an idl_name override
    if let Some(type_info) = import_ctx.find_type(type_ref) {
        if let Some(idl_name) = &type_info.idl_name {
            return Ok(idl_name.clone());
        }
    }

    // ... existing resolution logic ...
}
```

### 4. Update `#[idl_type]` Attribute Processing

When processing `#[idl_type("...")]` on instruction variants, ensure it can reference types by their `idl_name`:

```rust
fn rewrite_idl_type(
    idl_type_str: &str,
    import_ctx: &ImportContext,
) -> Result<IdlType> {
    // Parse the idl_type string and resolve type names
    // Use idl_name for matching if present
    // ...
}
```

## Example Usage

### Before (Current)

```rust
// In token_lib
#[derive(BorshDeserialize, BorshSerialize, ShankType)]
#[shank(import_from = "hyperlane_sealevel_connection_client", rename = "RemoteRouterConfig")]
pub struct RemoteRouterConfigProxy;

pub enum Instruction {
    #[idl_type("RemoteRouterConfigProxy")]
    EnrollRemoteRouter(RemoteRouterConfig),
}
```

**Generated IDL:**
```json
{
  "types": [
    { "name": "RemoteRouterConfigProxy", ... }
  ],
  "instructions": [
    {
      "name": "EnrollRemoteRouter",
      "args": [{ "type": { "defined": "RemoteRouterConfigProxy" } }]
    }
  ]
}
```

### After (With Enhancement)

```rust
// In token_lib
#[derive(BorshDeserialize, BorshSerialize, ShankType)]
#[shank(
    import_from = "hyperlane_sealevel_connection_client",
    rename = "RemoteRouterConfig",
    idl_name = "RemoteRouterConfig"  // Clean IDL name
)]
pub struct RemoteRouterConfigProxy;  // Clear Rust name

pub enum Instruction {
    #[idl_type("RemoteRouterConfig")]  // References idl_name
    EnrollRemoteRouter(RemoteRouterConfig),
}
```

**Generated IDL:**
```json
{
  "types": [
    { "name": "RemoteRouterConfig", ... }
  ],
  "instructions": [
    {
      "name": "EnrollRemoteRouter",
      "args": [{ "type": { "defined": "RemoteRouterConfig" } }]
    }
  ]
}
```

## Benefits

1. ✅ **Clean IDL output** - No `*Proxy` suffixes in generated files
2. ✅ **Clear Rust code** - Proxy types remain obviously marked
3. ✅ **No name collisions** - Proxy and real types coexist
4. ✅ **Backwards compatible** - Omitting `idl_name` preserves current behavior
5. ✅ **Explicit control** - Developers choose IDL names independently of Rust names

## Testing

Add test cases to `shank-idl/tests/fixtures/`:

### Test Case: Basic `idl_name` Usage

**lib.rs:**
```rust
#[derive(BorshDeserialize, BorshSerialize, ShankType)]
#[shank(
    import_from = "external_crate",
    rename = "ExternalType",
    idl_name = "ExternalType"
)]
pub struct ExternalTypeProxy;

#[derive(ShankInstruction)]
pub enum Instruction {
    #[idl_type("ExternalType")]
    DoSomething(external_crate::ExternalType),
}
```

**Expected IDL:**
```json
{
  "types": [
    { "name": "ExternalType", "type": { ... } }
  ],
  "instructions": [
    {
      "name": "DoSomething",
      "args": [{ "type": { "defined": "ExternalType" } }]
    }
  ]
}
```

### Test Case: Without `idl_name` (Backwards Compatibility)

**lib.rs:**
```rust
#[derive(ShankType)]
#[shank(import_from = "external", rename = "Type")]
pub struct TypeProxy;
```

**Expected IDL:**
```json
{
  "types": [
    { "name": "TypeProxy", "type": { ... } }  // Uses struct name
  ]
}
```

## Related Issues

- Cross-library type proxy resolution (implemented separately)
- Support for module paths in `#[idl_type]` (lower priority - can use `idl_name` instead)

## Implementation Complexity

**Estimated effort:** Small to medium
- Attribute parsing: ~1 hour
- IDL generation changes: ~2 hours
- Type resolution updates: ~2 hours
- Tests and edge cases: ~2 hours
- **Total: ~1 day**

## Priority

**Medium** - This is a quality-of-life improvement that makes generated IDLs cleaner for external consumers, but the current `*Proxy` suffix approach works functionally.
