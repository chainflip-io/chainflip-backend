//! Registry of hardware-attested WebAuthn credentials.
//!
//! A credential is admitted only if its `packed` attestation chains to a governance-pinned FIDO
//! issuing CA. Its account id is derived from the credential key, so nobody holds a native key for
//! that account. [`CheckWebAuthn`] lets registered credentials sign transactions.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod benchmarking;
mod extension;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;

pub use extension::CheckWebAuthn;
pub use pallet::*;
pub use weights::WeightInfo;

use alloc::vec::Vec;
use cf_webauthn::{Policy, PublicKey, MAX_CLIENT_DATA_JSON_LEN, MAX_CREDENTIAL_ID_LEN};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::*, BoundedVec};
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;
use sp_runtime::{traits::Zero, AccountId32};

const REGISTRATION_DOMAIN: &[u8] = b"chainflip/webauthn-register";
pub const MAX_ATTESTATION_OBJECT_LEN: u32 = 4096;
pub const MAX_CERTIFICATE_LEN: u32 = 2048;
pub const MAX_TRUST_ANCHORS: u32 = 8;
/// DNS names are at most 253 characters.
pub const MAX_RP_ID_LEN: u32 = 253;

pub type TrustAnchor = BoundedVec<u8, ConstU32<MAX_CERTIFICATE_LEN>>;

#[derive(
	Clone,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	RuntimeDebug,
)]
pub struct Credential {
	pub public_key: PublicKey,
	/// Needed by clients for `allowCredentials`, as security-key credentials are not
	/// discoverable.
	pub credential_id: BoundedVec<u8, ConstU32<MAX_CREDENTIAL_ID_LEN>>,
	pub aaguid: [u8; 16],
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<AccountId = AccountId32> {
		type EnsureGovernance: EnsureOrigin<Self::RuntimeOrigin>;
		type WeightInfo: WeightInfo;
	}

	#[pallet::storage]
	pub type RelyingPartyId<T> =
		StorageValue<_, BoundedVec<u8, ConstU32<MAX_RP_ID_LEN>>, ValueQuery>;

	/// DER certificates of the FIDO issuing CAs whose attestations are accepted.
	#[pallet::storage]
	pub type TrustAnchors<T> =
		StorageValue<_, BoundedVec<TrustAnchor, ConstU32<MAX_TRUST_ANCHORS>>, ValueQuery>;

	#[pallet::storage]
	pub type Credentials<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, Credential>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		CredentialRegistered { account_id: T::AccountId, aaguid: [u8; 16], sponsor: T::AccountId },
		RelyingPartyUpdated,
	}

	#[pallet::error]
	pub enum Error<T> {
		RelyingPartyNotConfigured,
		InvalidAttestation,
		AlreadyRegistered,
	}

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T> {
		pub rp_id: BoundedVec<u8, ConstU32<MAX_RP_ID_LEN>>,
		pub trust_anchors: BoundedVec<TrustAnchor, ConstU32<MAX_TRUST_ANCHORS>>,
		#[serde(skip)]
		pub _phantom: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			RelyingPartyId::<T>::put(&self.rp_id);
			TrustAnchors::<T>::put(&self.trust_anchors);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Registers the credential in a `navigator.credentials.create()` response. The
		/// ceremony's challenge must be [`Pallet::registration_challenge`] for the submitter,
		/// which binds the attestation to this chain and sponsor.
		///
		/// The submitter sponsors the new account: it has no native key, so it can't submit
		/// its own registration.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::register_credential())]
		pub fn register_credential(
			origin: OriginFor<T>,
			attestation_object: BoundedVec<u8, ConstU32<MAX_ATTESTATION_OBJECT_LEN>>,
			client_data_json: BoundedVec<u8, ConstU32<MAX_CLIENT_DATA_JSON_LEN>>,
		) -> DispatchResult {
			let sponsor = ensure_signed(origin)?;
			let policy = Self::policy().ok_or(Error::<T>::RelyingPartyNotConfigured)?;
			let trust_anchors = TrustAnchors::<T>::get();
			let registered = cf_webauthn::attestation::verify_registration(
				&attestation_object,
				&client_data_json,
				&Self::registration_challenge(&sponsor),
				&policy,
				&trust_anchors.iter().map(|anchor| anchor.as_slice()).collect::<Vec<_>>(),
			)
			.map_err(|_| Error::<T>::InvalidAttestation)?;

			let account_id = cf_webauthn::account_id(&registered.public_key);
			ensure!(!Credentials::<T>::contains_key(&account_id), Error::<T>::AlreadyRegistered);
			Credentials::<T>::insert(
				&account_id,
				Credential {
					public_key: registered.public_key,
					credential_id: registered
						.credential_id
						.try_into()
						.map_err(|_| Error::<T>::InvalidAttestation)?,
					aaguid: registered.aaguid,
				},
			);
			// `CheckNonce` rejects transactions from accounts that don't exist.
			frame_system::Pallet::<T>::inc_sufficients(&account_id);
			Self::deposit_event(Event::CredentialRegistered {
				account_id,
				aaguid: registered.aaguid,
				sponsor,
			});
			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(T::DbWeight::get().writes(2))]
		pub fn set_relying_party(
			origin: OriginFor<T>,
			rp_id: BoundedVec<u8, ConstU32<MAX_RP_ID_LEN>>,
			trust_anchors: BoundedVec<TrustAnchor, ConstU32<MAX_TRUST_ANCHORS>>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			RelyingPartyId::<T>::put(rp_id);
			TrustAnchors::<T>::put(trust_anchors);
			Self::deposit_event(Event::RelyingPartyUpdated);
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	pub fn registration_challenge(sponsor: &T::AccountId) -> [u8; 32] {
		(
			REGISTRATION_DOMAIN,
			frame_system::Pallet::<T>::block_hash(BlockNumberFor::<T>::zero()),
			sponsor,
		)
			.using_encoded(sp_io::hashing::blake2_256)
	}

	/// Policy applied to both registration and signing. Changing the RP id invalidates every
	/// registered credential, since authenticators scope credentials to it.
	pub fn policy() -> Option<Policy> {
		let rp_id = RelyingPartyId::<T>::get();
		(!rp_id.is_empty()).then(|| Policy {
			rp_id_hash: Some(cf_webauthn::rp_id_hash(&rp_id)),
			require_user_verification: false,
			reject_backup_eligible: true,
		})
	}
}
