# Shank Source Checkout

This is a fork of [metaplex-foundation/shank](https://github.com/metaplex-foundation/shank) — a Rust proc-macro tool that generates Anchor-compatible IDL from Solana program source code.

## Remotes
- `upstream` — metaplex-foundation/shank (original)
- `origin` — antigremlin/shank (our fork)
- `hl` — hyperlane-xyz/shank (Hyperlane org fork)

## Key branches
- `master` — tracks upstream
- `cursor/idl-imports-proxy-struct-5cdf` — our cross-crate IDL import feature
- `upstream/feat/better-discrim` — upstream discriminator work
- `upstream/thlorenz/account-discriminator` — upstream account discriminator work

## Source structure
- `shank_idl/` — IDL data structures and JSON serialization
- `shank_macro_impl/` — Core macro implementation (account, instruction parsing)
- `shank_macro/` — Proc macro entry points
- `shank_cli/` — CLI tool for IDL generation
