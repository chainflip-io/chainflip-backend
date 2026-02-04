// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{instances::PalletInstanceAlias, Chain, ChainCrypto, SetAggKeyWithAggKey};
use cf_primitives::EpochIndex;
use cf_runtime_utilities::EnumVariant;
use cf_traits::{
	AsyncResult, Broadcaster, CfeMultisigRequest, ChainflipWithTargetChain, CurrentEpochIndex,
	EpochTransitionHandler, GetBlockHeight, SafeMode, SetSafeMode, VaultKeyWitnessedHandler,
};
use cf_utilities::derive_common_traits_no_bounds;
use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;
use generic_typeinfo_derive::GenericTypeInfo;
pub use pallet::*;
use serde::{Deserialize, Serialize};
use sp_std::prelude::*;

mod benchmarking;
pub mod migrations;

mod vault_activator;

pub mod weights;
pub use weights::WeightInfo;
mod mock;
mod tests;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(5);

pub type PayloadFor<T, I = ()> =
	<<T as ChainflipWithTargetChain<I>>::TargetChain as ChainCrypto>::Payload;

pub type AggKeyFor<T, I = ()> =
	<<<T as ChainflipWithTargetChain<I>>::TargetChain as Chain>::ChainCrypto as ChainCrypto>::AggKey;
pub type ChainBlockNumberFor<T, I = ()> =
	<<T as ChainflipWithTargetChain<I>>::TargetChain as Chain>::ChainBlockNumber;
pub type TransactionInIdFor<T, I = ()> =
	<<<T as ChainflipWithTargetChain<I>>::TargetChain as Chain>::ChainCrypto as ChainCrypto>::TransactionInId;
pub type TransactionOutIdFor<T, I = ()> =
	<<<T as ChainflipWithTargetChain<I>>::TargetChain as Chain>::ChainCrypto as ChainCrypto>::TransactionOutId;
pub type ThresholdSignatureFor<T, I = ()> =
	<<<T as ChainflipWithTargetChain<I>>::TargetChain as Chain>::ChainCrypto as ChainCrypto>::ThresholdSignature;

/// The current status of a vault rotation.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebugNoBound, EnumVariant)]
#[scale_info(skip_type_params(T, I))]
pub enum VaultActivationStatus<T: Config<I>, I: 'static = ()> {
	/// We are waiting for the key to be updated on the contract, and witnessed by the network.
	AwaitingActivation { new_public_key: AggKeyFor<T, I> },
	/// The key has been successfully updated on the external chain, and/or funds rotated to new
	/// key.
	Complete,
	/// The activation tx has failed to construct. The rotation is now paused and awaiting
	/// governance.
	ActivationFailedAwaitingGovernance { new_public_key: AggKeyFor<T, I> },
}

#[frame_support::pallet]
pub mod pallet {

	use cf_traits::ChainflipWithTargetChain;

	use super::*;

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: ChainflipWithTargetChain<I> {
		/// The event type.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The supported api calls for the chain.
		type SetAggKeyWithAggKey: SetAggKeyWithAggKey<<Self::TargetChain as Chain>::ChainCrypto>;

		/// A broadcaster for the target chain.
		type Broadcaster: Broadcaster<Self::TargetChain, ApiCall = Self::SetAggKeyWithAggKey>;

		/// For activating Safe mode: CODE RED for the chain.
		type SafeMode: SafeMode + SetSafeMode<Self::SafeMode>;

		type ChainTracking: GetBlockHeight<Self::TargetChain>;

		type CfeMultisigRequest: CfeMultisigRequest<Self, <Self::TargetChain as Chain>::ChainCrypto>;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {}

	/// A map of starting block number of vaults by epoch.
	#[pallet::storage]
	#[pallet::getter(fn vault_start_block_numbers)]
	pub type VaultStartBlockNumbers<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, EpochIndex, ChainBlockNumberFor<T, I>>;

	/// Vault activation status for the current epoch rotation.
	#[pallet::storage]
	#[pallet::getter(fn pending_vault_rotations)]
	pub type PendingVaultActivation<T: Config<I>, I: 'static = ()> =
		StorageValue<_, VaultActivationStatus<T, I>>;

	/// Whether this chain is initialized.
	#[pallet::storage]
	#[pallet::getter(fn vault_initialized)]
	pub type ChainInitialized<T: Config<I>, I: 'static = ()> = StorageValue<_, bool, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// The vault for the request has rotated
		VaultActivationCompleted,
		/// The vault's key has been rotated externally \[new_public_key\]
		VaultRotatedExternally(<<T::TargetChain as Chain>::ChainCrypto as ChainCrypto>::AggKey),
		/// The new key has been generated, we must activate the new key on the external
		/// chain via governance.
		AwaitingGovernanceActivation {
			new_public_key: <<T::TargetChain as Chain>::ChainCrypto as ChainCrypto>::AggKey,
		},
		ActivationTxFailedAwaitingGovernance {
			new_public_key: <<T::TargetChain as Chain>::ChainCrypto as ChainCrypto>::AggKey,
		},
		ChainInitialized,
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// There is currently no vault rotation in progress for this chain.
		NoActiveRotation,
		/// The requested call is invalid based on the current rotation state.
		InvalidRotationStatus,
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// The vault's key has been updated externally, outside of the rotation
		/// cycle. This is an unexpected event as far as our chain is concerned, and
		/// the only thing we can do is to update the vault key to be sure we can continue
		/// to properly sign transactions.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::vault_key_rotated_externally())]
		pub fn vault_key_rotated_externally(
			origin: OriginFor<T>,
			new_public_key: AggKeyFor<T, I>,
			block_number: ChainBlockNumberFor<T, I>,
			tx_id: TransactionInIdFor<T, I>,
		) -> DispatchResult {
			T::EnsureWitnessedAtCurrentEpoch::ensure_origin(origin)?;

			Self::inner_vault_key_rotated_externally(VaultKeyRotatedExternally {
				new_public_key,
				block_number,
				tx_id,
			});

			Ok(())
		}

		/// Sets the ChainInitialized flag to true for this chain so that the chain can be
		/// initialized on the next epoch rotation
		#[pallet::call_index(5)]
		// This weight is not strictly correct but since it's a governance call, weight is
		// irrelevant.
		#[pallet::weight(T::WeightInfo::initialize_chain())]
		pub fn initialize_chain(origin: OriginFor<T>) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			ChainInitialized::<T, I>::put(true);

			Self::deposit_event(Event::<T, I>::ChainInitialized);

			Ok(())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		pub deployment_block: Option<ChainBlockNumberFor<T, I>>,
		pub chain_initialized: bool,
	}

	impl<T: Config<I>, I: 'static> Default for GenesisConfig<T, I> {
		fn default() -> Self {
			Self { deployment_block: None, chain_initialized: true }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> BuildGenesisConfig for GenesisConfig<T, I> {
		fn build(&self) {
			if let Some(deployment_block) = self.deployment_block {
				VaultStartBlockNumbers::<T, I>::insert(
					cf_primitives::GENESIS_EPOCH,
					<T::TargetChain as Chain>::block_witness_root(deployment_block),
				);
			} else {
				log::info!("No genesis vault key configured for {}.", Pallet::<T, I>::name());
			}
			ChainInitialized::<T, I>::put(self.chain_initialized);
		}
	}
}

derive_common_traits_no_bounds! {
	#[derive_where(PartialOrd, Ord; )]
	#[derive(GenericTypeInfo)]
	#[expand_name_with(<T::TargetChain as PalletInstanceAlias>::TYPE_INFO_SUFFIX)]
	pub struct VaultKeyRotatedExternally<T: Config<I>, I: 'static> {
		pub new_public_key: AggKeyFor<T, I>,
		pub block_number: ChainBlockNumberFor<T, I>,
		pub tx_id: TransactionInIdFor<T, I>
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	fn activate_new_key_for_chain(block_number: ChainBlockNumberFor<T, I>) {
		PendingVaultActivation::<T, I>::put(VaultActivationStatus::<T, I>::Complete);
		VaultStartBlockNumbers::<T, I>::insert(
			CurrentEpochIndex::<T>::get().saturating_add(1),
			<T::TargetChain as Chain>::saturating_block_witness_next(block_number),
		);
		Self::deposit_event(Event::VaultActivationCompleted);
	}

	pub fn inner_vault_key_rotated_externally(
		VaultKeyRotatedExternally { new_public_key, block_number, tx_id: _ }: VaultKeyRotatedExternally<T, I>,
	) {
		Self::activate_new_key_for_chain(block_number);

		Pallet::<T, I>::deposit_event(Event::VaultRotatedExternally(new_public_key));
	}
}

impl<T: Config<I>, I: 'static> VaultKeyWitnessedHandler<T::TargetChain> for Pallet<T, I> {
	fn on_first_key_activated(block_number: ChainBlockNumberFor<T, I>) -> DispatchResult {
		let rotation =
			PendingVaultActivation::<T, I>::get().ok_or(Error::<T, I>::NoActiveRotation)?;

		ensure_variant!(
			VaultActivationStatus::<T, I>::AwaitingActivation { new_public_key } => new_public_key,
			rotation,
			Error::<T, I>::InvalidRotationStatus
		);

		Self::activate_new_key_for_chain(block_number);

		Ok(())
	}

	#[cfg(feature = "runtime-benchmarks")]
	/// Setup states for a successful key activation - used for benchmarking only.
	fn setup_key_activation() {
		PendingVaultActivation::<T, I>::put(VaultActivationStatus::<T, I>::AwaitingActivation {
			new_public_key: cf_chains::benchmarking_value::BenchmarkValue::benchmark_value(),
		});
	}
}

impl<T: Config<I>, I: 'static> EpochTransitionHandler for Pallet<T, I> {
	fn on_expired_epoch(expired_epoch: EpochIndex) {
		for epoch in VaultStartBlockNumbers::<T, I>::iter_keys()
			.filter(|epoch| *epoch <= expired_epoch)
			.collect::<Vec<EpochIndex>>()
		{
			VaultStartBlockNumbers::<T, I>::remove(epoch);
		}
	}
}

/// Takes three arguments: a pattern, a variable expression and an error literal.
///
/// If the variable matches the pattern, returns it, otherwise returns an error. The pattern may
/// optionally have an expression attached to process and return inner arguments.
///
/// ## Example
///
/// let x = ensure_variant!(Some(..), optional_value, Error::<T>::ValueIsNone);
///
/// let 2x = ensure_variant!(Some(x) => { 2 * x }, optional_value, Error::<T>::ValueIsNone);
#[macro_export]
macro_rules! ensure_variant {
	( $variant:pat => $varexp:expr, $var:expr, $err:expr $(,)? ) => {
		if let $variant = $var {
			$varexp
		} else {
			frame_support::fail!($err)
		}
	};
	( $variant:pat, $var:expr, $err:expr $(,)? ) => {
		ensure_variant!($variant => { $var }, $var, $err)
	};
}
