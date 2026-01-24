# Shank IDL Import Test Cases

These isolated test cases are designed to drive the implementation of "Option 1 (Proxy Struct)" from the `TODO-idl-imports.md` design document. They represent the final refined pattern for handling IDL imports in Hyperlane Sealevel programs, ensuring that compiled code remains identical while the generated IDL is correctly enriched with external type definitions.

Each case consists of an `exporting-lib` (the source of truth) and an `importing-program` (the consumer).

## The Pattern: Proxy Metadata

The consistent pattern used across these tests involves three components:
1. **The Real Type:** Imported directly from the library and used in the program's logic and data structures.
2. **The Proxy Struct:** A zero-sized struct (or empty enum) annotated with `#[shank(import_from = "...", rename = "...")]` which provides the metadata for the IDL generator.
3. **The Annotation:** Using `#[idl_type("ProxyName")]` on fields or variants to tell Shank to resolve the type using the proxy's import metadata instead of the real type's (potentially invisible) local definition.

---

## Case 1: Simple Type Proxy
**Paths:**
- `case1/exporting-lib/src/lib.rs` (defines `RemoteRouterConfig`)
- `case1/importing-program/src/lib.rs` (uses real type with a proxy)

**Scenario:** Replaces the manual duplication of structs like `RemoteRouterConfig`. The program uses the real struct from the library but points Shank to the proxy for IDL generation.

## Case 2: Account State & Type Preservation
**Paths:**
- `case2/exporting-lib/src/lib.rs` (defines `IncrementalMerkle`)
- `case2/importing-program/src/lib.rs` (uses real `IncrementalMerkle` in a `ShankAccount`)

**Scenario:** The most critical case for data layout preservation. The program uses the library's `IncrementalMerkle` inside its `Outbox` account. This ensures method calls like `tree.ingest()` work natively. The `MerkleTreeProxy` ensures the IDL correctly describes the complex internal structure of the tree by importing it from the library's IDL.

## Case 3: Instruction Enum Proxy
**Paths:**
- `case3/exporting-lib/src/lib.rs` (defines a shared `Instruction` enum)
- `case3/importing-program/src/lib.rs` (uses a proxy enum to re-export instructions)

**Scenario:** Handles shared instruction sets. The program logic may operate on a library-defined enum, but it needs those variants to appear in the program's own IDL. The `TokenInstructionProxy` maps the entire enum and its associated data types (`Init`, `TransferRemote`) from the library.

---

## Expected Shank Behavior

1. **Proxy Discovery:** Shank identifies items with `#[derive(ShankType)]` or `#[derive(ShankInstruction)]` that carry the `import_from` and `rename` attributes.
2. **Mapping Table:** Shank builds a mapping of `LocalProxyName` -> `(ExternalCrate, OriginalName)`.
3. **Indirect Resolution:** When Shank encounters a field or variant with `#[idl_type("ProxyName")]`, it bypasses normal type analysis and looks up the definition in the mapping table.
4. **IDL Merging:** During the final generation phase, the Shank CLI:
    - Locates the IDL files for the crates specified in `import_from`.
    - Extracts the types/instructions matching the `rename` (or `LocalProxyName` if no rename is provided).
    - Injects these definitions into the `types` and `instructions` sections of the resulting IDL.