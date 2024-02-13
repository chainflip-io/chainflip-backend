#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{Chain, ChainCrypto, SetAggKeyWithAggKey};
use cf_primitives::EpochIndex;
use cf_runtime_utilities::EnumVariant;
use cf_traits::{
	AsyncResult, Broadcaster, CfeMultisigRequest, Chainflip, CurrentEpochIndex, GetBlockHeight,
	SafeMode, SetSafeMode, VaultKeyWitnessedHandler,
};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::traits::{One, Saturating},
	traits::StorageVersion,
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_std::prelude::*;

mod benchmarking;
pub mod migrations;

mod vault_activator;

pub mod weights;
pub use weights::WeightInfo;
mod mock;
mod tests;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(4);

pub type PayloadFor<T, I = ()> = <<T as Config<I>>::Chain as ChainCrypto>::Payload;

pub type AggKeyFor<T, I = ()> =
	<<<T as Config<I>>::Chain as Chain>::ChainCrypto as ChainCrypto>::AggKey;
pub type ChainBlockNumberFor<T, I = ()> = <<T as Config<I>>::Chain as Chain>::ChainBlockNumber;
pub type TransactionInIdFor<T, I = ()> =
	<<<T as Config<I>>::Chain as Chain>::ChainCrypto as ChainCrypto>::TransactionInId;
pub type TransactionOutIdFor<T, I = ()> =
	<<<T as Config<I>>::Chain as Chain>::ChainCrypto as ChainCrypto>::TransactionOutId;
pub type ThresholdSignatureFor<T, I = ()> =
	<<<T as Config<I>>::Chain as Chain>::ChainCrypto as ChainCrypto>::ThresholdSignature;

/// The current status of a vault rotation.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebugNoBound, EnumVariant)]
#[scale_info(skip_type_params(T, I))]
pub enum VaultActivationStatus<T: Config<I>, I: 'static = ()> {
	/// We are waiting for the key to be updated on the contract, and witnessed by the network.
	AwaitingActivation { new_public_key: AggKeyFor<T, I> },
	/// The key has been successfully updated on the external chain, and/or funds rotated to new
	/// key.
	Complete,
}

#[frame_support::pallet]
pub mod pallet {

	use super::*;

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// The event type.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The chain that is managed by this vault must implement the api types.
		type Chain: Chain;

		/// The supported api calls for the chain.
		type SetAggKeyWithAggKey: SetAggKeyWithAggKey<
			<<Self as pallet::Config<I>>::Chain as Chain>::ChainCrypto,
		>;

		/// A broadcaster for the target chain.
		type Broadcaster: Broadcaster<Self::Chain, ApiCall = Self::SetAggKeyWithAggKey>;

		/// For activating Safe mode: CODE RED for the chain.
		type SafeMode: SafeMode + SetSafeMode<Self::SafeMode>;

		type ChainTracking: GetBlockHeight<Self::Chain>;

		type CfeMultisigRequest: CfeMultisigRequest<Self, <Self::Chain as Chain>::ChainCrypto>;

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

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// The vault for the request has rotated
		VaultActivationCompleted,
		/// The vault's key has been rotated externally \[new_public_key\]
		VaultRotatedExternally(<<T::Chain as Chain>::ChainCrypto as ChainCrypto>::AggKey),
		/// The new key has been generated, we must activate the new key on the external
		/// chain via governance.
		AwaitingGovernanceActivation {
			new_public_key: <<T::Chain as Chain>::ChainCrypto as ChainCrypto>::AggKey,
		},
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
		/// the only thing we can do is to halt and wait for further governance
		/// intervention.
		///
		/// This function activates CODE RED for the runtime's safe mode, which halts
		/// many functions on the statechain.
		///
		/// ## Events
		///
		/// - [VaultRotatedExternally](Event::VaultRotatedExternally)
		///
		/// ## Errors
		///
		/// - None
		///
		/// ## Dependencies
		///
		/// - [Epoch Info Trait](EpochInfo)
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::vault_key_rotated_externally())]
		pub fn vault_key_rotated_externally(
			origin: OriginFor<T>,
			new_public_key: AggKeyFor<T, I>,
			block_number: ChainBlockNumberFor<T, I>,
			_tx_id: TransactionInIdFor<T, I>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessedAtCurrentEpoch::ensure_origin(origin)?;

			Self::activate_new_key_for_chain(block_number);

			T::SafeMode::set_code_red();

			Pallet::<T, I>::deposit_event(Event::VaultRotatedExternally(new_public_key));

			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		pub deployment_block: Option<ChainBlockNumberFor<T, I>>,
	}

	impl<T: Config<I>, I: 'static> Default for GenesisConfig<T, I> {
		fn default() -> Self {
			Self { deployment_block: None }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> BuildGenesisConfig for GenesisConfig<T, I> {
		fn build(&self) {
			if let Some(deployment_block) = self.deployment_block {
				VaultStartBlockNumbers::<T, I>::insert(
					cf_primitives::GENESIS_EPOCH,
					deployment_block,
				);
			} else {
				log::info!("No genesis vault key configured for {}.", Pallet::<T, I>::name());
			}
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	fn activate_new_key_for_chain(block_number: ChainBlockNumberFor<T, I>) {
		PendingVaultActivation::<T, I>::put(VaultActivationStatus::<T, I>::Complete);
		VaultStartBlockNumbers::<T, I>::insert(
			CurrentEpochIndex::<T>::get().saturating_add(1),
			block_number.saturating_add(One::one()),
		);
		Self::deposit_event(Event::VaultActivationCompleted);
	}
}

impl<T: Config<I>, I: 'static> VaultKeyWitnessedHandler<T::Chain> for Pallet<T, I> {
	fn on_first_key_activated(
		block_number: ChainBlockNumberFor<T, I>,
	) -> DispatchResultWithPostInfo {
		let rotation =
			PendingVaultActivation::<T, I>::get().ok_or(Error::<T, I>::NoActiveRotation)?;

		ensure_variant!(
			VaultActivationStatus::<T, I>::AwaitingActivation { new_public_key } => new_public_key,
			rotation,
			Error::<T, I>::InvalidRotationStatus
		);

		Self::activate_new_key_for_chain(block_number);

		Ok(().into())
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
