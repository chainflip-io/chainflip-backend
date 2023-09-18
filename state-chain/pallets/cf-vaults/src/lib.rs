#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{Chain, ChainCrypto, SetAggKeyWithAggKey};
use cf_primitives::{AuthorityCount, CeremonyId, EpochIndex, ThresholdSignatureRequestId};
use cf_runtime_utilities::{EnumVariant, StorageDecodeVariant};
use cf_traits::{
	impl_pallet_safe_mode, offence_reporting::OffenceReporter, AccountRoleRegistry, AsyncResult,
	Broadcaster, Chainflip, CurrentEpochIndex, EpochKey, GetBlockHeight, KeyProvider, KeyState,
	SafeMode, SetSafeMode, Slashing, ThresholdSigner, VaultKeyWitnessedHandler, VaultRotator,
	VaultTransitionHandler,
};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::traits::{One, Saturating},
	traits::StorageVersion,
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	iter::Iterator,
	prelude::*,
};

mod benchmarking;

mod vault_rotator;

mod response_status;

use response_status::ResponseStatus;

pub mod weights;
pub use weights::WeightInfo;
mod mock;
mod tests;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

const KEYGEN_CEREMONY_RESPONSE_TIMEOUT_BLOCKS_DEFAULT: u32 = 90;

pub type PayloadFor<T, I = ()> = <<T as Config<I>>::Chain as ChainCrypto>::Payload;
pub type KeygenOutcomeFor<T, I = ()> =
	Result<AggKeyFor<T, I>, BTreeSet<<T as Chainflip>::ValidatorId>>;
pub type AggKeyFor<T, I = ()> =
	<<<T as Config<I>>::Chain as Chain>::ChainCrypto as ChainCrypto>::AggKey;
pub type ChainBlockNumberFor<T, I = ()> = <<T as Config<I>>::Chain as Chain>::ChainBlockNumber;
pub type TransactionInIdFor<T, I = ()> =
	<<<T as Config<I>>::Chain as Chain>::ChainCrypto as ChainCrypto>::TransactionInId;
pub type TransactionOutIdFor<T, I = ()> =
	<<<T as Config<I>>::Chain as Chain>::ChainCrypto as ChainCrypto>::TransactionOutId;
pub type ThresholdSignatureFor<T, I = ()> =
	<<<T as Config<I>>::Chain as Chain>::ChainCrypto as ChainCrypto>::ThresholdSignature;

pub type KeygenResponseStatus<T, I> =
	ResponseStatus<T, KeygenSuccessVoters<T, I>, KeygenFailureVoters<T, I>, I>;

pub type KeyHandoverResponseStatus<T, I> =
	ResponseStatus<T, KeyHandoverSuccessVoters<T, I>, KeyHandoverFailureVoters<T, I>, I>;

impl_pallet_safe_mode!(PalletSafeMode; slashing_enabled);

/// The current status of a vault rotation.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug, EnumVariant)]
#[scale_info(skip_type_params(T, I))]
pub enum VaultActivationStatus<T: Config<I>, I: 'static = ()> {
	/// We are waiting for the key to be updated on the contract, and witnessed by the network.
	AwaitingActivation { new_public_key: AggKeyFor<T, I> },
	/// The key has been successfully updated on the external chain, and/or funds rotated to new
	/// key.
	Complete,
}

impl<T: Config<I>, I: 'static> cf_traits::CeremonyIdProvider for Pallet<T, I> {
	fn increment_ceremony_id() -> CeremonyId {
		CeremonyIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		})
	}
}

/// A single vault.
#[derive(Default, PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
pub struct Vault<T: Chain> {
	/// The vault's public key.
	pub public_key: <<T as Chain>::ChainCrypto as ChainCrypto>::AggKey,
	/// The first active block for this vault
	pub active_from_block: T::ChainBlockNumber,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletOffence {
	FailedKeygen,
	FailedKeyHandover,
}

#[frame_support::pallet]
pub mod pallet {
	use frame_support::sp_runtime::Percent;

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

		/// Ensure that only threshold signature consensus can trigger a key_verification success
		type EnsureThresholdSigned: EnsureOrigin<Self::RuntimeOrigin>;

		/// Offences supported in this runtime.
		type Offence: From<PalletOffence>;

		/// The chain that is managed by this vault must implement the api types.
		type Chain: Chain;

		/// The supported api calls for the chain.
		type SetAggKeyWithAggKey: SetAggKeyWithAggKey<
			<<Self as pallet::Config<I>>::Chain as Chain>::ChainCrypto,
		>;

		type VaultTransitionHandler: VaultTransitionHandler<Self::Chain>;

		/// The pallet dispatches calls, so it depends on the runtime's aggregated Call type.
		type RuntimeCall: From<Call<Self, I>> + IsType<<Self as frame_system::Config>::RuntimeCall>;

		type ThresholdSigner: ThresholdSigner<
			<Self::Chain as Chain>::ChainCrypto,
			Callback = <Self as Config<I>>::RuntimeCall,
			ValidatorId = Self::ValidatorId,
		>;

		/// A broadcaster for the target chain.
		type Broadcaster: Broadcaster<Self::Chain, ApiCall = Self::SetAggKeyWithAggKey>;

		/// For reporting misbehaviour
		type OffenceReporter: OffenceReporter<
			ValidatorId = Self::ValidatorId,
			Offence = Self::Offence,
		>;

		type Slasher: Slashing<AccountId = Self::ValidatorId, BlockNumber = BlockNumberFor<Self>>;

		/// For activating Safe mode: CODE RED for the chain.
		type SafeMode: Get<PalletSafeMode> + SafeMode + SetSafeMode<Self::SafeMode>;

		type ChainTracking: GetBlockHeight<Self::Chain>;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		fn on_runtime_upgrade() -> Weight {
			// For new pallet instances, genesis items need to be set.
			if !KeygenResponseTimeout::<T, I>::exists() {
				KeygenResponseTimeout::<T, I>::set(
					KEYGEN_CEREMONY_RESPONSE_TIMEOUT_BLOCKS_DEFAULT.into(),
				);
			}
			Weight::zero()
		}
	}

	/// A map of vaults by epoch.
	#[pallet::storage]
	#[pallet::getter(fn vaults)]
	pub type Vaults<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, EpochIndex, Vault<T::Chain>>;

	/// Counter for generating unique ceremony ids.
	#[pallet::storage]
	#[pallet::getter(fn ceremony_id_counter)]
	pub type CeremonyIdCounter<T: Config<I>, I: 'static = ()> =
		StorageValue<_, CeremonyId, ValueQuery>;

	/// Vault activation statuses for the current epoch rotation.
	#[pallet::storage]
	#[pallet::getter(fn pending_vault_rotations)]
	pub type PendingVaultActivation<T: Config<I>, I: 'static = ()> =
		StorageValue<_, VaultActivationStatus<T, I>>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// The vault for the request has rotated
		VaultRotationCompleted,
		/// The vault's key has been rotated externally \[new_public_key\]
		VaultRotatedExternally(<<T::Chain as Chain>::ChainCrypto as ChainCrypto>::AggKey),
		/// The new key has been generated, we must activate the new key on the external
		/// chain via governance.
		AwaitingGovernanceActivation {
			new_public_key: <<T::Chain as Chain>::ChainCrypto as ChainCrypto>::AggKey,
		},
		/// Key handover has failed
		KeyHandoverFailure { ceremony_id: CeremonyId },
		/// The vault rotation has been aborted early.
		VaultRotationAborted,
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// An invalid ceremony id
		InvalidCeremonyId,
		/// There is currently no vault rotation in progress for this chain.
		NoActiveRotation,
		/// The requested call is invalid based on the current rotation state.
		InvalidRotationStatus,
		/// An authority sent a response for a ceremony in which they weren't involved, or to which
		/// they have already submitted a response.
		InvalidRespondent,
		/// There is no threshold signature available
		ThresholdSignatureUnavailable,
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// A vault rotation event has been witnessed, we update the vault with the new key.
		///
		/// ## Events
		///
		/// - [VaultRotationCompleted](Event::VaultRotationCompleted)
		///
		/// ## Errors
		///
		/// - [NoActiveRotation](Error::NoActiveRotation)
		/// - [InvalidRotationStatus](Error::InvalidRotationStatus)
		///
		/// ## Dependencies
		///
		/// - [Epoch Info Trait](EpochInfo)
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::vault_key_rotated())]
		pub fn vault_key_rotated(
			origin: OriginFor<T>,
			block_number: ChainBlockNumberFor<T, I>,

			// This field is primarily required to ensure the witness calls are unique per
			// transaction (on the external chain)
			_tx_id: TransactionInIdFor<T, I>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessedAtCurrentEpoch::ensure_origin(origin)?;

			Self::on_new_key_activated(block_number)
		}

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

			Self::activate_new_key(new_public_key, block_number);

			T::SafeMode::set_code_red();

			Pallet::<T, I>::deposit_event(Event::VaultRotatedExternally(new_public_key));

			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		pub vault_key: Option<AggKeyFor<T, I>>,
		pub deployment_block: ChainBlockNumberFor<T, I>,
		pub keygen_response_timeout: BlockNumberFor<T>,
	}

	impl<T: Config<I>, I: 'static> Default for GenesisConfig<T, I> {
		fn default() -> Self {
			use frame_support::sp_runtime::traits::Zero;
			Self {
				vault_key: None,
				deployment_block: Zero::zero(),
				keygen_response_timeout: KEYGEN_CEREMONY_RESPONSE_TIMEOUT_BLOCKS_DEFAULT.into(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> BuildGenesisConfig for GenesisConfig<T, I> {
		fn build(&self) {
			if let Some(vault_key) = self.vault_key {
				Pallet::<T, I>::set_vault_key_for_epoch(
					cf_primitives::GENESIS_EPOCH,
					Vault { public_key: vault_key, active_from_block: self.deployment_block },
				);
			} else {
				log::info!("No genesis vault key configured for {}.", Pallet::<T, I>::name());
			}

			KeygenResponseTimeout::<T, I>::put(self.keygen_response_timeout);
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	fn activate_new_key(new_agg_key: AggKeyFor<T, I>, block_number: ChainBlockNumberFor<T, I>) {
		PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Complete);
		Self::set_vault_key_for_epoch(
			CurrentEpochIndex::<T>::get().saturating_add(1),
			Vault {
				public_key: new_agg_key,
				active_from_block: block_number.saturating_add(One::one()),
			},
		);
		T::VaultTransitionHandler::on_new_vault();
		Self::deposit_event(Event::VaultRotationCompleted);
	}
}

impl<T: Config<I>, I: 'static> VaultKeyWitnessedHandler<T::Chain> for Pallet<T, I> {
	fn on_new_key_activated(block_number: ChainBlockNumberFor<T, I>) -> DispatchResultWithPostInfo {
		let rotation =
			PendingVaultRotation::<T, I>::get().ok_or(Error::<T, I>::NoActiveRotation)?;

		let new_public_key = ensure_variant!(
			VaultRotationStatus::<T, I>::AwaitingActivation { new_public_key } => new_public_key,
			rotation,
			Error::<T, I>::InvalidRotationStatus
		);

		// Unlock the key that was used to authorise the activation, *if* this was triggered via
		// broadcast (as opposed to governance, for example).
		// TODO: use broadcast callbacks for this.
		CurrentVaultEpochAndState::<T, I>::try_mutate(|state: &mut Option<VaultEpochAndState>| {
			state
				.as_mut()
				.map(|VaultEpochAndState { key_state, .. }| key_state.unlock())
				.ok_or(())
		})
		.unwrap_or_else(|_| {
			log::info!(
				"No key to unlock for {}. This is expected if the rotation was triggered via governance.",
				T::Chain::NAME,
			);
		});

		Self::activate_new_key(new_public_key, block_number);

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
