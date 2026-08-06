---
name: writing-migrations
description: Use when writing, reviewing, or removing a storage migration for a state-chain pallet — bumping a pallet storage version, adding a VersionedMigration, or verifying an upgrade with try-runtime. Triggered by changes to pallet storage layout or requests like "write a migration".
---

# Writing Storage Migrations

## Structure

Each pallet has a `migrations.rs` that defines a `PalletMigration<T>` type alias as a tuple of `VersionedMigration`s, ending with a `PlaceholderMigration`:

```rust
pub type PalletMigration<T> = (
    VersionedMigration<N, N+1, my_migration::Migration<T>, Pallet<T>, <T as frame_system::Config>::DbWeight>,
    PlaceholderMigration<CURRENT_VERSION, Pallet<T>>,
);
```

Individual migrations live in `migrations/my_migration.rs` and implement `UncheckedOnRuntimeUpgrade`.

## Checklist

When writing a migration:

1. **Bump `STORAGE_VERSION_U16`** in the pallet's `lib.rs` (the `StorageVersion::new(N)` constant).
2. **Add the migration module** under `migrations/` implementing `UncheckedOnRuntimeUpgrade`.
3. **Update `PalletMigration`** in `migrations.rs`: add a new `VersionedMigration` entry and update the `PlaceholderMigration` version.
4. **Define old storage types** using `#[frame_support::storage_alias]` in an `old` module within the migration file. This avoids depending on types that may change.
5. **Implement `pre_upgrade` and `post_upgrade`** (gated behind `#[cfg(feature = "try-runtime")]`) to verify migration correctness.
6. **For instanced pallets** (e.g. `cf-broadcast`, `cf-ingress-egress`), ensure all instances are migrated. Use `NoopRuntimeUpgrade` for instances that don't need data changes.
7. **Runtime-level migrations** (cross-pallet, one-off) go in `state-chain/runtime/src/migrations/` and implement `OnRuntimeUpgrade` directly. These need to be explicitly added to the runtime migrations: they not automatically included in the same way as pallet migrations.
8. **Test with try-runtime** before merging.

## Placeholder Migration

Always keep a `PlaceholderMigration<VERSION, Pallet<T>>` pointing at the current version to keep the boilerplate consistent and to surface inconsistencies in the pallet storage versions.

## Related

A migration template lives at `state-chain/runtime-utilities/src/migration_template.rs`. If the change also alters the wire shape of a `CustomRuntimeApi` method, see the "Runtime API Versioning" section of the root CLAUDE.md.
