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

2. Add a migrations module and pallet `StorageVersion`  to the pallet's `lib.rs` file if you haven't done so already:

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

4. Now create `migrations/my_migration.rs` with an implemtation of `OnRuntimeUpgrade`:

    ```rust
    use crate::*;
    use sp_std::marker::PhantomData;

    /// My first migration.
    pub struct Migration<T: Config>(PhantomData<T>);

    impl<T: Config> OnRuntimeUpgrade for Migration<T> {
        fn on_runtime_upgrade() -> frame_support::weights::Weight {
            todo!()
        }

        #[cfg(feature = "try-runtime")]
        fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
            todo!()
        }

        #[cfg(feature = "try-runtime")]
        fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
            todo!()
        }
    }
    ```

5. Copy the following boilerplate into the pallet hooks:

    ```rust
        #[pallet::hooks]
        impl<T: Config> Hooks<T::BlockNumber> for Pallet<T> {
            // [...]

            fn on_runtime_upgrade() -> Weight {
                migrations::PalletMigration::<T>::on_runtime_upgrade()
            }

            #[cfg(feature = "try-runtime")]
            fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
                migrations::PalletMigration::<T>::pre_upgrade()
            }

            #[cfg(feature = "try-runtime")]
            fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
                migrations::PalletMigration::<T>::post_upgrade(state)
            }
        }
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
