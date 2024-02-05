# Runtime Upgrade Utilities

This crate provides a `VersionedMigration` type that can be used to structure successive pallet storage migrations.

## Versioned Pallet Migrations

1. Add this crate to the pallet's `Cargo.toml`.

   ```toml
   [dependencies]
   # ...
   cf-runtime-upgrade-utilities = { path = '../../runtime-upgrade-utilities', default-features = false }

   [features]

   std = [
       # ...
       'cf-runtime-upgrade-utilities/std',
   ]
   try-runtime = [
       # ...
       'cf-runtime-upgrade-utilities/try-runtime'
   ]
   ```

2. Add a migrations module and pallet `StorageVersion` to the pallet's `lib.rs` file if you haven't done so already:

   ```rust
   mod migrations; // <--- We will create this module file next

   // These imports are required if not already present.
   use frame_support::traits::{OnRuntimeUpgrade, StorageVersion};

   // Bump this if already present.
   pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

   #[frame_support::pallet]
   pub mod pallet {
       // [...]

       #[pallet::pallet]
       #[pallet::storage_version(PALLET_VERSION)] // <-- Add this if not already present.
       // [...]
       pub struct Pallet<T>(_);

       // [...]
   }
   ```

3. Create `migrations.rs` based on the following template:

   ```rust
   pub mod my_migration;

   use cf_runtime_upgrade_utilities::VersionedMigration;

   pub type PalletMigration<T> =
       (VersionedMigration<crate::Pallet<T>, my_migration::Migration<T>, 0, 1>,);
   ```

4. Now create `migrations/my_migration.rs` with an implementation of `OnRuntimeUpgrade`. You can use the `src/migration_template.rs` included in this crate as a starting point.

5. If this is the first migration for this pallet, ensure that the `PalletMigration` for this pallet is added to the tuple of PalletMigrations in `state-chain/runtime/src/lib.rs`. Remember to add all the pallet's instances!

   ```rust
   type PalletMigrations = (
     pallet_cf_environment::migrations::PalletMigration<Runtime>,
        pallet_cf_funding::migrations::PalletMigration<Runtime>,
        pallet_cf_validator::migrations::PalletMigration<Runtime>,
        pallet_cf_governance::migrations::PalletMigration<Runtime>,
        pallet_cf_tokenholder_governance::migrations::PalletMigration<Runtime>,
        pallet_cf_threshold_signature::migrations::PalletMigration<Runtime, Instance1>,
        pallet_cf_threshold_signature::migrations::PalletMigration<Runtime, Instance2>,
        pallet_cf_threshold_signature::migrations::PalletMigration<Runtime, Instance3>,
        pallet_cf_broadcast::migrations::PalletMigration<Runtime, Instance1>,
        pallet_cf_broadcast::migrations::PalletMigration<Runtime, Instance2>,
        pallet_cf_broadcast::migrations::PalletMigration<Runtime, Instance3>,
        pallet_cf_chain_tracking::migrations::PalletMigration<Runtime, Instance1>,
        pallet_cf_chain_tracking::migrations::PalletMigration<Runtime, Instance2>,
        pallet_cf_chain_tracking::migrations::PalletMigration<Runtime, Instance3>,
        pallet_cf_vaults::migrations::PalletMigration<Runtime, Instance1>,
        pallet_cf_vaults::migrations::PalletMigration<Runtime, Instance2>,
        pallet_cf_vaults::migrations::PalletMigration<Runtime, Instance3>,
        pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, Instance1>,
        pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, Instance2>,
        pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, Instance3>,
        pallet_cf_swapping::migrations::PalletMigration<Runtime>,
        pallet_cf_lp::migrations::PalletMigration<Runtime>,
   );
   ```

6. Additional migrations can be added to the `PalletMigration` tuple. For example, the following defines migrations from version 0 through 4. Only the required migrations will be applied on-chain. For example, if the on-chain storage version is 2 and the pallet version is 4, the migrations `change_storage_type_b` and `purge_old_values` would be run, and the on-chain storage version would be updated to 4.

   ```rust
   pub mod rename_pallet_storage;
   pub mod change_storage_type_a;
   pub mod change_storage_type_b;
   pub mod purge_old_values;

   type PalletMigration<T> = (
       VersionedMigration<crate::Pallet<T>, rename_pallet_storage::Migration, 0, 1>,
       VersionedMigration<crate::Pallet<T>, change_storage_type_a::Migration, 1, 2>,
       VersionedMigration<crate::Pallet<T>, change_storage_type_b::Migration, 2, 3>,
       VersionedMigration<crate::Pallet<T>, purge_old_values::Migration, 3, 4>,
   );
   ```
