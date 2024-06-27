pub use cf_primitives::chains::Solana;

use cf_primitives::ChannelId;
use sp_core::ConstBool;
use sp_std::vec::Vec;

use sol_prim::{AccountBump, SlotNumber};

use crate::{address, assets, DepositChannel, FeeEstimationApi, FeeRefundCalculator, TypeInfo};
use codec::{Decode, Encode, MaxEncodedLen};
use serde::{Deserialize, Serialize};

use super::{Chain, ChainCrypto};

pub mod api;
pub mod benchmarking;
pub mod instruction_builder;
pub mod sol_tx_core;

pub use crate::assets::sol::Asset as SolAsset;
pub use sol_prim::{
	pda::{Pda as DerivedAddressBuilder, PdaError as AddressDerivationError},
	Address as SolAddress, Amount as SolAmount, ComputeLimit as SolComputeLimit, Digest as SolHash,
	Signature as SolSignature,
};
pub use sol_tx_core::{
	AccountMeta as SolAccountMeta, CcmAccounts as SolCcmAccounts, CcmAddress as SolCcmAddress,
	Hash as RawSolHash, Instruction as SolInstruction, Message as SolMessage, Pubkey as SolPubkey,
	Transaction as SolTransaction,
};

impl Chain for Solana {
	const NAME: &'static str = "Solana";
	const GAS_ASSET: Self::ChainAsset = assets::sol::Asset::Sol;
	const WITNESS_PERIOD: Self::ChainBlockNumber = 15;

	type ChainCrypto = SolanaCrypto;
	type ChainBlockNumber = SlotNumber;
	type ChainAmount = SolAmount;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = SolTrackedData;
	type ChainAsset = assets::sol::Asset;
	type ChainAccount = SolAddress;
	type DepositFetchId = SolanaDepositFetchId;
	type DepositChannelState = AccountBump;
	type DepositDetails = (); //todo
	type Transaction = SolTransaction;
	type TransactionMetadata = (); //todo
	type ReplayProtectionParams = (); //todo
	type ReplayProtection = (); //todo
	type TransactionRef = SolSignature;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SolanaCrypto;

impl ChainCrypto for SolanaCrypto {
	type UtxoChain = ConstBool<false>;
	type KeyHandoverIsRequired = ConstBool<false>;

	type AggKey = SolAddress;
	type Payload = SolMessage;
	type ThresholdSignature = SolSignature;
	type TransactionInId = SolHash;
	type TransactionOutId = Self::ThresholdSignature;

	type GovKey = SolAddress;

	fn verify_threshold_signature(
		agg_key: &Self::AggKey,
		payload: &Self::Payload,
		signature: &Self::ThresholdSignature,
	) -> bool {
		use sp_core::ed25519::{Public, Signature};
		use sp_io::crypto::ed25519_verify;

		ed25519_verify(
			&Signature::from_raw(signature.0),
			payload.serialize().as_slice(),
			&Public::from_raw(agg_key.0),
		)
	}

	fn agg_key_to_payload(agg_key: Self::AggKey, _for_handover: bool) -> Self::Payload {
		SolMessage::new(&[], Some(&SolPubkey::from(agg_key)))
	}

	fn maybe_broadcast_barriers_on_rotation(
		_rotation_broadcast_id: cf_primitives::BroadcastId,
	) -> Vec<cf_primitives::BroadcastId> {
		todo!()
	}
}

pub const LAMPORTS_PER_SIGNATURE: SolAmount = 5000u64;

// This is to be used both for ingress/egress estimation and for setting the compute units
// limit when crafting transactions by the State Chain.
mod compute_units_costs {
	use super::SolAmount;
	pub const BASE_COMPUTE_UNITS_PER_TX: SolAmount = 450u64;
	pub const COMPUTE_UNITS_PER_FETCH_NATIVE: SolAmount = 7_500u64;
	pub const COMPUTE_UNITS_PER_TRANSFER_NATIVE: SolAmount = 300u64;
	#[allow(dead_code)]
	pub const COMPUTE_UNITS_PER_FETCH_TOKEN: SolAmount = 31_000u64;
	#[allow(dead_code)]
	pub const COMPUTE_UNITS_PER_TRANSFER_TOKEN: SolAmount = 41_200u64;
}

#[derive(
	Default,
	Clone,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	Debug,
	PartialEq,
	Eq,
	Serialize,
	Deserialize,
)]
pub struct SolTrackedData {
	pub priority_fee: <Solana as Chain>::ChainAmount,
}

impl FeeEstimationApi<Solana> for SolTrackedData {
	fn estimate_egress_fee(
		&self,
		asset: <Solana as crate::Chain>::ChainAsset,
	) -> <Solana as crate::Chain>::ChainAmount {
		use compute_units_costs::*;

		let compute_units_per_transfer = BASE_COMPUTE_UNITS_PER_TX +
			match asset {
				assets::sol::Asset::Sol => COMPUTE_UNITS_PER_TRANSFER_NATIVE,
				assets::sol::Asset::SolUsdc => COMPUTE_UNITS_PER_TRANSFER_TOKEN,
			};

		LAMPORTS_PER_SIGNATURE + (self.priority_fee).saturating_mul(compute_units_per_transfer)
	}
	fn estimate_ingress_fee(
		&self,
		asset: <Solana as crate::Chain>::ChainAsset,
	) -> <Solana as crate::Chain>::ChainAmount {
		use compute_units_costs::*;

		let compute_units_per_fetch = BASE_COMPUTE_UNITS_PER_TX +
			match asset {
				assets::sol::Asset::Sol => COMPUTE_UNITS_PER_FETCH_NATIVE,
				assets::sol::Asset::SolUsdc => COMPUTE_UNITS_PER_FETCH_TOKEN,
			};

		LAMPORTS_PER_SIGNATURE + (self.priority_fee).saturating_mul(compute_units_per_fetch)
	}
}

impl FeeRefundCalculator<Solana> for SolTransaction {
	fn return_fee_refund(
		&self,
		fee_paid: <Solana as Chain>::TransactionFee,
	) -> <Solana as Chain>::ChainAmount {
		fee_paid
	}
}

impl TryFrom<address::ForeignChainAddress> for SolAddress {
	type Error = address::AddressError;
	fn try_from(value: address::ForeignChainAddress) -> Result<Self, Self::Error> {
		if let address::ForeignChainAddress::Sol(value) = value {
			Ok(value)
		} else {
			Err(address::AddressError::InvalidAddress)
		}
	}
}
impl From<SolAddress> for address::ForeignChainAddress {
	fn from(value: SolAddress) -> Self {
		address::ForeignChainAddress::Sol(value)
	}
}

impl address::ToHumanreadableAddress for SolAddress {
	#[cfg(feature = "std")]
	type Humanreadable = String;

	#[cfg(feature = "std")]
	fn to_humanreadable(
		&self,
		_network_environment: cf_primitives::NetworkEnvironment,
	) -> Self::Humanreadable {
		self.to_string()
	}
}

impl crate::ChannelLifecycleHooks for AccountBump {}

#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Copy, Debug)]
pub struct SolanaDepositFetchId {
	pub channel_id: ChannelId,
	pub address: SolAddress,
	pub bump: AccountBump,
}

impl From<&DepositChannel<Solana>> for SolanaDepositFetchId {
	fn from(from: &DepositChannel<Solana>) -> Self {
		SolanaDepositFetchId {
			channel_id: from.channel_id,
			address: from.address,
			bump: from.state,
		}
	}
}

#[cfg(any(test, feature = "runtime-integration-tests"))]
pub mod signing_key {
	use crate::sol::{
		sol_tx_core::signer::{Signer, SignerError},
		SolPubkey, SolSignature,
	};
	use ed25519_dalek::Signer as DalekSigner;
	use rand::{rngs::OsRng, CryptoRng, RngCore};

	#[derive(Clone)]
	pub struct SolSigningKey(ed25519_dalek::SigningKey);

	impl SolSigningKey {
		/// Constructs a new, random `Keypair` using a caller-provided RNG
		pub fn generate<R>(csprng: &mut R) -> Self
		where
			R: CryptoRng + RngCore,
		{
			Self(ed25519_dalek::SigningKey::generate(csprng))
		}

		/// Constructs a new random `Keypair` using `OsRng`
		pub fn new() -> Self {
			let mut rng = OsRng;
			Self::generate(&mut rng)
		}

		/// Recovers a `SolSigningKey` from a byte array
		pub fn from_bytes(bytes: &[u8]) -> Result<Self, ed25519_dalek::SignatureError> {
			Ok(Self(ed25519_dalek::SigningKey::from_bytes(
				<&[_; ed25519_dalek::SECRET_KEY_LENGTH]>::try_from(bytes).map_err(|_| {
					ed25519_dalek::SignatureError::from_source(String::from(
						"candidate keypair byte array is the wrong length",
					))
				})?,
			)))
		}

		/// Returns this `SolSigningKey` as a byte array
		pub fn to_bytes(&self) -> [u8; ed25519_dalek::SECRET_KEY_LENGTH] {
			self.0.to_bytes()
		}

		/// Gets this `SolSigningKey`'s SecretKey
		pub fn secret(&self) -> &ed25519_dalek::SigningKey {
			&self.0
		}
	}

	impl Signer for SolSigningKey {
		#[inline]
		fn pubkey(&self) -> SolPubkey {
			SolPubkey::from(ed25519_dalek::VerifyingKey::from(&self.0).to_bytes())
		}

		fn try_pubkey(&self) -> Result<SolPubkey, SignerError> {
			Ok(self.pubkey())
		}

		fn sign_message(&self, message: &[u8]) -> SolSignature {
			SolSignature::from(self.0.sign(message).to_bytes())
		}

		fn try_sign_message(&self, message: &[u8]) -> Result<SolSignature, SignerError> {
			Ok(self.sign_message(message))
		}

		fn is_interactive(&self) -> bool {
			false
		}
	}
}
