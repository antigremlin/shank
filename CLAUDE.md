# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Shank is a collection of Rust crates that extract Interface Definition Language (IDL) files from Solana programs using macro annotations. The generated IDL is consumed by tools like [solita](https://github.com/metaplex-foundation/solita) to generate TypeScript SDKs for Solana programs.

## Architecture

This is a Rust workspace containing 6 main crates:

- **shank** - Top-level crate that exports all macros, entry point for users
- **shank-macro** - Provides derive macros (`ShankAccount`, `ShankInstruction`, `ShankType`, etc.)
- **shank-macro-impl** - Core implementation of the derive macros and parsing logic
- **shank-idl** - Processes Rust source files to extract IDL from shank annotations
- **shank-render** - Generates Rust code (like PDA functions) from annotations
- **shank-cli** - Command-line tool that orchestrates IDL extraction

The workflow: Users annotate their Solana program structs/enums with shank macros → shank-cli analyzes the source code → produces JSON IDL → consumed by code generators.

## Common Commands

### Building and Testing
```bash
cargo test                              # Run all tests across workspace
cargo test -p shank-idl                 # Run tests for specific crate
cargo test account_from_single_file     # Run specific test by name
cargo build                             # Build all crates
cargo build --release                   # Release build
```

### Running Individual Crate Tests
```bash
cargo test -p shank-macro-impl          # Test macro implementation
cargo test -p shank-idl                 # Test IDL extraction
cargo test -p shank-render              # Test code generation
```

### Updating Test Fixtures
```bash
UPDATE_IDL=1 cargo test                 # Regenerate expected IDL JSON files
```

Tests use fixture-based testing with golden JSON files in `tests/fixtures/`. When test logic changes, set `UPDATE_IDL=1` to regenerate expected outputs.

### CLI Usage
```bash
cargo install shank-cli                 # Install CLI globally
shank idl                               # Extract IDL to ./idl/ directory
shank idl -o <dir>                      # Extract IDL to custom directory
shank idl -r <crate-root>               # Specify program crate root
shank idl --out-filename custom.json    # Custom output filename
shank idl -p <program-id>               # Override program address
```

### Release Process
Only from `master` branch:
```bash
cargo test && cargo release <major|minor|patch>     # Dry run
cargo release <major|minor|patch> --execute         # Execute release
```

## Key Macro Annotations

- `#[derive(ShankAccount)]` - Marks account structs with optional `#[seeds]` for PDA generation
- `#[derive(ShankInstruction)]` - Marks instruction enums with `#[account]` attributes
- `#[derive(ShankType)]` - Marks custom types for IDL inclusion
- `#[derive(ShankBuilder)]` - Generates instruction builders 
- `#[derive(ShankContext)]` - Generates account context structs

### Field Attributes

- `#[padding]` - Marks field as padding in IDL
- `#[idl_type("TypeName")]` - Overrides field type in IDL
- `#[idl_name("name")]` - Renames field in IDL while keeping Rust field name
- `#[skip]` - Excludes field from IDL entirely

## Architecture Deep Dive

### Type System Flow
The type conversion pipeline is central to Shank's operation:

1. **Rust AST (syn)** → Parsed by `syn` crate
2. **RustType** (in `shank-macro-impl/src/types/`) → Internal representation bridging AST and IDL
   - `TypeKind` classifies types: Primitive, Value, Composite, Custom
   - `resolve_rust_ty.rs` converts syn types to RustType
3. **IdlType** (in `shank-idl/src/idl_type.rs`) → JSON-serializable IDL representation
   - Handles special cases like `PodOption` with sentinel values
   - Supports nested types: Option, Vec, Array, HashMap, etc.

### Macro Processing Pipeline
1. **Compile-time**: `shank-macro` receives `TokenStream` → delegates to `shank-macro-impl`
2. `shank-macro-impl` parses attributes and extracts metadata
3. `shank-render` generates code (PDA methods, builders, contexts) using `quote!`
4. **Runtime**: `shank-cli` invokes `shank-idl` to extract IDL from compiled source
5. `shank-idl` uses `CrateContext` to navigate full AST and build IDL structure

### Key Modules
- `shank-macro-impl/src/parsed_struct/` - Struct parsing with seeds and field attributes
- `shank-macro-impl/src/instruction/` - Instruction enum parsing with account metadata
- `shank-idl/src/file.rs` - Main orchestration of IDL extraction (~15k lines)
- `shank-render/src/pda/` - PDA seed and address generation

### PodOption Handling
Special fixed-size optional type for Solana programs:
- Primitives use MAX values as sentinels (e.g., `u8::MAX = 0xFF`)
- Pubkey uses all-zeros
- Custom types can specify via `#[pod_sentinel(255, ...)]`
- Automatically generates sentinel values in IDL

## Testing

### Test Organization
- `shank-idl/tests/`: Integration tests for IDL extraction
  - `accounts.rs` - Account struct tests
  - `instructions.rs` - Instruction enum tests
  - `types.rs` - Custom type tests
  - `errors.rs` - Error enum tests
- `tests/fixtures/` - Contains `.rs` source fixtures and `.json` expected outputs
- Macro crates use inline unit tests and doc tests

### Test Pattern
Tests use `check_or_update_idl()` pattern:
- Normal run: Compare generated IDL against golden JSON
- `UPDATE_IDL=1`: Regenerate expected JSON files when logic changes

## Development Notes

- Uses Rust 2018 edition
- Release configuration in `release.toml`
- Only releases from `master` branch
- Uses `rustfmt.toml` for consistent formatting
- Heavy use of `syn` (AST parsing) and `quote` (code generation) for macro implementation
- IDL format based on Anchor's IDL with Shank-specific extensions