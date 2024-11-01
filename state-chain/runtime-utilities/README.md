# Utilities for Runtime upgrades

This crate provides a `NoopRuntimeUpgrade` and `PlaceholderMigration` that supplements Substrate's `frame_support::migrations::VersionedMigration` to provide developers with tools to perform runtime storage migrations.

## Versioned migration 

Some migrations are run as part of a "Versioned" upgrade - it should be run exactly once, for a specific version. Such migrations are done with Substrate's `VersionedMigration` tool. 

``` rust,ignore
   VersionedMigration<FROM, TO, Inner, Pallet, DbWeight>
```

Where `FROM` is the exact pallet version that this migration should run.

`TO` is the version to set the pallet version after the migration.

`Inner` the code for the actual upgrade implementation. Inner implementation must implements the `UncheckedOnRuntimeUpgrade` trait.

`Pallet` is usually the Substrate Pallet defined in the crate.

`DbWeight` is the amount of weight this migration will consume.

## Standalone migrations

Other migrations are standalone migrations where the migration is expected to run without constraints to the target Pallet's storage version. These standalone migrations do not need to use Substrate's `VersionedMigration` and should implement `OnRuntimeUpgrade` directly.

Migration type        | Trait to implement
--------------------- | --------------------
Versioned migration   | UncheckedOnRuntimeUpgrade
Standalone migration  | VersionedMigration

## Placeholder migrations

When removing migrations, it can be helpful to leave a placeholder migration to avoid deleting the boilerplate. For example, after removing old migrations we should leave a placeholder pointing at the latest pallet version, like this:

   ```rust,ignore
   use cf_runtime_utilities::PlaceholderMigration;

   type PalletMigration<T> = PlaceholderMigration<4, crate::Pallet<T>>;
   ```

## Noop migrations

For instance pallets (e.g. Pallet::<Runtime, EthereumInstance>), sometimes the runtime migrations are not required for all instances. For example, we may be updating storage for Ethereum chain only and the migration will not affect the same Pallet's other chain's instances. You can use `NoopRuntimeUpgrade` do ensure all other instances of the pallets are migrated to the same version. For example - in the example below we upgrade SolanaInstance of the Broadcaster pallet from version 1 to 3, we also want to use `NoopRuntimeUpgrade` to update all other instances to version 3.

``` rust,ignore
	VersionedMigration<1, 2, Migration1,
		pallet_cf_broadcast::Pallet<Runtime, SolanaInstance>,
		DbWeight,
	>,
	VersionedMigration<2, 3, Migration2,
		pallet_cf_broadcast::Pallet<Runtime, SolanaInstance>,
		DbWeight,
	>,

   // Update other instances to version 3
	VersionedMigration<1, 3, NoopRuntimeUpgrade,
		pallet_cf_broadcast::Pallet<Runtime, EthereumInstance>,
		DbWeight,
	>,
	VersionedMigration<1, 3, NoopRuntimeUpgrade,
		pallet_cf_broadcast::Pallet<Runtime, PolkadotInstance>,
		DbWeight,
	>,
	VersionedMigration<1, 3, NoopRuntimeUpgrade,
		pallet_cf_broadcast::Pallet<Runtime, BitcoinInstance>,
		DbWeight,
	>,
	VersionedMigration<1, 3, NoopRuntimeUpgrade,
		pallet_cf_broadcast::Pallet<Runtime, ArbitrumInstance>,
		DbWeight,
	>,
```

## Examples with code

1. Add this crate to the pallet's `Cargo.toml`.

   ```toml
   [dependencies]
   # ...
   cf-runtime-utilities = { workspace = true }

   [features]
   std = [
       # ...
       'cf-runtime-utilities/std',
   ]
   try-runtime = [
       # ...
       'cf-runtime-utilities/try-runtime',
   ]
   runtime-benchmarks = [
      # ...
      'cf-runtime-utilities/runtime-benchmarks',
   ]
   ```

2. Add a migrations module and pallet `StorageVersion` to the pallet's `lib.rs` file if you haven't done so already:

   ```rust,ignore
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

   ```rust,ignore
   pub mod my_migration;

   use frame_support::migrations::VersionedMigration;

   pub type PalletMigration<T> =
       (VersionedMigration<0, 1, my_migration::Migration<T>, crate::Pallet<T>, <T as frame_system::Config>::DbWeight);
   ```

4. Now create `migrations/my_migration.rs` with an implementation of `frame_support::trait::UncheckedOnRuntimeUpgrade`. You can use the `src/migration_template.rs` included in this crate as a starting point.

5. If this is the first migration for this pallet, ensure that the `PalletMigration` for this pallet is added to the tuple of PalletMigrations in `state-chain/runtime/src/lib.rs`. Remember to add all the pallet's instances!

   ```rust,ignore
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

   ```rust,ignore
   pub mod rename_pallet_storage;
   pub mod change_storage_type_a;
   pub mod change_storage_type_b;
   pub mod purge_old_values;

   type PalletMigration<T> = (
      VersionedMigration<0, 1, rename_pallet_storage::Migration, crate::Pallet<T>, <T as frame_system::Config>::DbWeight>,
      VersionedMigration<1, 2, change_storage_type_a::Migration, crate::Pallet<T>, <T as frame_system::Config>::DbWeight>,
      VersionedMigration<2, 3, change_storage_type_b::Migration, crate::Pallet<T>, <T as frame_system::Config>::DbWeight>,
      VersionedMigration<3, 4, purge_old_values::Migration, crate::Pallet<T>, <T as frame_system::Config>::DbWeight>,
   );
   ```
