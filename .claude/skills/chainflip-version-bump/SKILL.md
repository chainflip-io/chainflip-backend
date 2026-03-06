---
name: chainflip-version-bump
description: Use when bumping the Chainflip runtime version on the (e.g. 2.1 to 2.2), removing old migrations, and updating engine/runtime versions. Triggered by phrases like "version bump", "bump to X.Y", "remove old migrations", "prepare next release".
---

# Chainflip Version Bump

Bump all crate versions, clean up old migrations, and update engine versioning for a new major/minor release.

**This applies to major/minor version bumps on `main` only (e.g. 2.1 -> 2.2). Patch version bumps (e.g. 2.1.0 -> 2.1.1) happen on `release/*` branches and do NOT follow this process.**

## Overview

A major/minor version bump (e.g. 2.1 -> 2.2) involves three coordinated changes:

1. **Cargo.toml versions** across ~13 crates
2. **Engine versioning** (old/new dylib references)
3. **Runtime migrations** (clean up old VersionedMigrations, reset the release-specific migration tuple)

Reference commits: `b4ae1f3e2b` (2.0->2.1), and the 2.1->2.2 bump done in this repo.

## Commit Strategy

Split into two commits:

1. **Version bumps** - Cargo.toml versions, runtime `spec_version`, engine old/new versions, `Cargo.lock` (steps 1-3, 7)
2. **Migration cleanup & CI cleanup** - Remove old VersionedMigrations, clean up pallet migrations, delete migration files, remove temporary CI workarounds (steps 4-6)

## Step-by-Step Process

### 1. Bump Cargo.toml Versions

Find all crates at the old version and bump them:

```bash
grep -r '^version = "OLD_VER"' --include='Cargo.toml' -l
```

Crates that need version bumps (may vary - search to confirm):

- `api/bin/chainflip-broker-api/Cargo.toml`
- `api/bin/chainflip-cli/Cargo.toml`
- `api/bin/chainflip-lp-api/Cargo.toml`
- `api/lib/Cargo.toml`
- `engine/Cargo.toml`
- `engine/p2p/Cargo.toml`
- `engine/sc-client/Cargo.toml`
- `engine-dylib/Cargo.toml` (version AND `[lib] name`)
- `engine-proc-macros/Cargo.toml`
- `engine-runner-bin/Cargo.toml` (version AND `.so` asset paths)
- `state-chain/node/Cargo.toml`
- `state-chain/runtime/Cargo.toml`

### 2. Update Engine Versioning

Three files encode the old/new engine version relationship:

**`engine-upgrade-utils/src/lib.rs`:**

```rust
pub const OLD_VERSION: &str = "OLD_VER";  // becomes current version
pub const NEW_VERSION: &str = "NEW_VER";  // becomes the new version
```

**`engine-runner-bin/src/main.rs`:**

```rust
mod old {
    #[engine_proc_macros::link_engine_library_version("OLD_VER")]  // update
    // ...
}
mod new {
    #[engine_proc_macros::link_engine_library_version("NEW_VER")]  // update
    // ...
}
```

**`engine-dylib/Cargo.toml`:**

```toml
name = "chainflip_engine_vX_Y_Z"  # underscores, matches NEW_VER
```

**`engine-runner-bin/Cargo.toml`** assets section:

- New version `.so` paths use NEW_VER
- Old version `.so` paths shift to what was previously the new version

### 3. Bump Runtime spec_version

In `state-chain/runtime/src/lib.rs`:

```rust
spec_version: X_YY_00,  // e.g. 2_01_00 -> 2_02_00
```

### 4. Clean Up Runtime Migrations

**`state-chain/runtime/src/lib.rs` - AllMigrations:**

- Remove any release-specific migrations from `AllMigrations` (entries like `migrations::some_migration::Migration`)
- Rename `MigrationsForVX_Y` to `MigrationsForVX_Z = ()`
- Keep permanent entries: `ClearEvents`, `VersionUpdate`, `PalletMigrations`, `housekeeping::Migration`, anything else explicitly marked "Do not remove".

**`state-chain/runtime/src/migrations.rs` (module root):**

- Remove `pub mod` declarations for deleted migration files
- Keep `pub mod housekeeping;`

**Delete old runtime migration files** from `state-chain/runtime/src/migrations/`:

- Delete all `.rs` files except `housekeeping.rs`
- Keep the `housekeeping/` subdirectory intact

The `VersionedMigration` import and `instanced_migrations!` macro in `lib.rs` are kept for future use. Add `#[allow(unused_imports)]` to the import if needed.

### 5. Clean Up Pallet Migrations

For each pallet that has `VersionedMigration` entries in its `migrations.rs`:

**Before:**

```rust
use frame_support::migrations::VersionedMigration;
mod old_migration;

pub type PalletMigration<T> = (
    VersionedMigration<N, M, old_migration::Migration<T>, Pallet<T>, ...>,
    PlaceholderMigration<M, Pallet<T>>,
);
```

**After:**

```rust
use cf_runtime_utilities::PlaceholderMigration;

pub type PalletMigration<T> = (PlaceholderMigration<M, Pallet<T>>,);
```

Then delete the old migration sub-module files from `pallets/*/src/migrations/`.

**Important exceptions:**

- `cf-elections`: Keep `vote_storage_migration::VoteStorageMigration` (comment says "Keep this migration")
- `cf-environment`: Keep the `VersionUpdate` struct and its test (used by runtime's `AllMigrations`)
- `cf-governance`: If it has a `VersionUpdate`, check if it's used externally before removing
- Anything else explicitly marked "Do not remove" in the code/comments.

Search for all pallets with VersionedMigration:

```bash
grep -r 'VersionedMigration' state-chain/pallets/*/src/migrations.rs
```

### 6. Clean Up Temporary CI Workarounds

Check GitHub Actions workflows for temporary changes marked for removal after the previous release:

```bash
grep -rn 'TODO.*temporary\|TODO.*[Rr]emove after\|TODO.*workaround' .github/workflows/
```

These are typically `sed` commands, extra steps, or patched values that were needed to bridge compatibility between versions during upgrade tests. Remove any that reference the version you're bumping *from* (e.g. "Remove after 2.1 is released" when bumping from 2.1 to 2.2).

### 7. Update Cargo.lock

```bash
cargo generate-lockfile
```

### 8. Verify Compilation

```bash
cargo check -p state-chain-runtime
cargo check -p engine-runner
```

### 9. Test with try-runtime

You need a snapshot from a chain running the **previous** version (the one whose migrations you just cleaned up). Check mainnet first; if the previous version isn't on mainnet yet, fall back to testnet (Sisyphos).

Query spec versions to determine which network to use:

```bash
# Mainnet
curl https://mainnet-rpc.chainflip.io \
  -H 'Content-Type: application/json' -X POST \
  -d '{"jsonrpc":"2.0","id":1,"method":"state_getRuntimeVersion","params":[]}' \
  -s | jq '.result.specVersion'

# Sisyphos (testnet) - use if previous version isn't on mainnet yet
curl https://archive.sisyphos.chainflip.io \
  -H 'Content-Type: application/json' -X POST \
  -d '{"jsonrpc":"2.0","id":1,"method":"state_getRuntimeVersion","params":[]}' \
  -s | jq '.result.specVersion'
```

Check for existing snapshots, or create one from the appropriate network:

```bash
ls chainflip-node-*.snap

# From mainnet (if previous version is live there)
try-runtime create-snapshot --uri=wss://mainnet-rpc.chainflip.io

# From Sisyphos (if previous version is only on testnet)
try-runtime create-snapshot --uri=wss://archive.sisyphos.chainflip.io
```

Build with try-runtime and test:

```bash
cargo build --release --features=try-runtime
try-runtime \
  --runtime ./target/release/wbuild/state-chain-runtime/state_chain_runtime.compact.compressed.wasm \
  on-runtime-upgrade \
  --blocktime=6000 \
  --disable-spec-version-check \
  --disable-mbm-checks \
  --checks pre-and-post \
  snap --path ./chainflip-node-XXXXX@latest.snap
```

**Expected result for a version bump:** Storage version mismatch errors are expected when testing against a snapshot that hasn't run the previous version's migrations yet (e.g. mainnet still on 2.0 when bumping from 2.1 to 2.2). The errors should correspond exactly to the pallets whose `VersionedMigration`s were cleaned up. If testing against a network that *has* run the previous migrations, the test should pass cleanly.

## Migration System Reference

### Migration Types

| Type        | Trait                        | Use Case                                                     |
| ----------- | ---------------------------- | ------------------------------------------------------------ |
| Versioned   | `UncheckedOnRuntimeUpgrade`  | Run exactly once for a specific pallet version               |
| Standalone  | `OnRuntimeUpgrade`           | Run without pallet version constraints                       |
| Placeholder | `PlaceholderMigration<N, P>` | Marker after removing old versioned migrations               |
| Noop        | `NoopRuntimeUpgrade`         | Bump version for instanced pallets that don't need migration |

### AllMigrations Structure

```rust
type AllMigrations = (
    // 1. Clear CFE events (always first)
    pallet_cf_cfe_interface::migrations::ClearEvents<Runtime>,
    // 2. Update on-chain version for CFE compatibility (DO NOT REMOVE)
    pallet_cf_environment::migrations::VersionUpdate<Runtime>,
    // 3. Per-pallet migrations (managed at pallet level)
    PalletMigrations,
    // 4. Housekeeping (network-specific cleanup)
    migrations::housekeeping::Migration,
    // 5. Release-specific migrations (cleared each version bump)
    MigrationsForVX_Y,
);
```

### instanced_migrations! Macro

For migrations that apply to some chain instances but not others:

```rust
instanced_migrations! {
    module: pallet_cf_ingress_egress,
    migration: MyMigration,
    from: 29,
    to: 30,
    include_instances: [EthereumInstance, ArbitrumInstance],
    exclude_instances: [PolkadotInstance, BitcoinInstance, SolanaInstance, AssethubInstance],
}
```

Uses `NoopRuntimeUpgrade` for excluded instances to bump their version without running migration logic.

## Common Mistakes

- Forgetting to update `engine-dylib/Cargo.toml` lib name (uses underscores: `chainflip_engine_vX_Y_Z`)
- Forgetting to shift the old engine `.so` paths in `engine-runner-bin/Cargo.toml`
- Removing `VersionUpdate` from `cf-environment` migrations (it's marked "Do not remove")
- Removing `VoteStorageMigration` from `cf-elections` (it's marked "Keep this migration")
- Not checking for new pallets added since the last bump that may also have VersionedMigration entries
- Leaving dead `pub mod` declarations in `migrations.rs` after deleting files
