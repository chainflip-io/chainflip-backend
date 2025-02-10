pub use cf_primitives::chains::Solana;

use cf_primitives::ChannelId;
use sp_core::ConstBool;
use sp_std::{vec, vec::Vec};

use sol_prim::{AccountBump, SlotNumber};

use crate::{
	address, assets, DepositChannel, DepositDetailsToTransactionInId, FeeEstimationApi,
	FeeRefundCalculator, TypeInfo,
};
use codec::{Decode, Encode, FullCodec, MaxEncodedLen};
use frame_support::{sp_runtime::RuntimeDebug, Parameter};
use serde::{Deserialize, Serialize};
use sp_runtime::traits::Member;

use super::{Chain, ChainCrypto};

pub mod api;
pub mod benchmarking;
pub mod instruction_builder;
pub mod sol_tx_core;
pub mod transaction_builder;

pub use crate::assets::sol::Asset as SolAsset;
use crate::benchmarking_value::BenchmarkValue;
pub use sol_prim::{
	consts::{
		LAMPORTS_PER_SIGNATURE, MAX_TRANSACTION_LENGTH, MICROLAMPORTS_PER_LAMPORT,
		TOKEN_ACCOUNT_RENT,
	},
	pda::{Pda as DerivedAddressBuilder, PdaError as AddressDerivationError},
	Address as SolAddress, Amount as SolAmount, ComputeLimit as SolComputeLimit, Digest as SolHash,
	Signature as SolSignature, SlotNumber as SolBlockNumber,
};
pub use sol_tx_core::{
	rpc_types, AccountMeta as SolAccountMeta, CcmAccounts as SolCcmAccounts,
	CcmAddress as SolCcmAddress, Hash as RawSolHash, Instruction as SolInstruction,
	InstructionRpc as SolInstructionRpc, LegacyMessage as SolLegacyMessage,
	LegacyTransaction as SolLegacyTransaction, Pubkey as SolPubkey,
};

// Due to transaction size limit in Solana, we have a limit on number of fetches in a solana fetch
// tx. Batches of 5 fetches get to ~1000 bytes, max ~1090 for tokens.
pub const MAX_SOL_FETCHES_PER_TX: usize = 5;

// Bytes left that are available for the user when building the native and token ccm transfers.
// All function parameters are already accounted except additional_accounts and message.
pub const MAX_CCM_BYTES_SOL: usize = MAX_TRANSACTION_LENGTH - 538usize + 32usize; // 694 bytes left + 32 empty source address
pub const MAX_CCM_BYTES_USDC: usize = MAX_TRANSACTION_LENGTH - 751usize + 32usize; // 481 bytes left + 32 empty source address

// Nonce management values
pub const NONCE_NUMBER_CRITICAL_NONCES: usize = 1;
pub const NONCE_AVAILABILITY_THRESHOLD_FOR_INITIATING_TRANSFER: usize = 1;

// Values used when closing vault swap accounts.
pub const MAX_BATCH_SIZE_OF_VAULT_SWAP_ACCOUNT_CLOSURES: usize = 5;
pub const MAX_WAIT_BLOCKS_FOR_SWAP_ACCOUNT_CLOSURE_APICALLS: u32 = 14400;
pub const NONCE_AVAILABILITY_THRESHOLD_FOR_INITIATING_SWAP_ACCOUNT_CLOSURES: usize = 3;

// Use serialized transaction
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub struct SolanaTransactionData {
	pub serialized_transaction: Vec<u8>,
	pub skip_preflight: bool,
}

/// A Solana transaction in id is a tuple of the AccountAddress and the slot number.
pub type SolanaTransactionInId = (SolAddress, u64);

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
	type ChainAssetMap<
		T: Member + Parameter + MaxEncodedLen + Copy + BenchmarkValue + FullCodec + Unpin,
	> = assets::sol::AssetMap<T>;
	type ChainAccount = SolAddress;
	type DepositFetchId = SolanaDepositFetchId;
	type DepositChannelState = AccountBump;
	type DepositDetails = ();
	type Transaction = SolanaTransactionData;
	type TransactionMetadata = ();
	// There is no need for replay protection on Solana since it uses blockhashes.
	type ReplayProtectionParams = ();
	type ReplayProtection = ();
	type TransactionRef = SolSignature;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SolanaCrypto;

impl ChainCrypto for SolanaCrypto {
	const NAME: &'static str = "Solana";
	type UtxoChain = ConstBool<false>;
	type KeyHandoverIsRequired = ConstBool<false>;

	type AggKey = SolAddress;
	type Payload = SolLegacyMessage;
	type ThresholdSignature = SolSignature;
	type TransactionInId = SolanaTransactionInId;
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
		SolLegacyMessage::new(&[], Some(&SolPubkey::from(agg_key)))
	}

	fn maybe_broadcast_barriers_on_rotation(
		rotation_broadcast_id: cf_primitives::BroadcastId,
	) -> Vec<cf_primitives::BroadcastId> {
		// In solana, we need all the txs to have successfully broadcasted before we can broadcast
		// the rotation tx so that all the durable nonces have been consumed. Moreover, we also need
		// to wait for the rotation tx to go through before broadcasting subsequent txs. In both
		// cases, failure to do so will fail transactions.
		if rotation_broadcast_id > 1 {
			vec![rotation_broadcast_id - 1, rotation_broadcast_id]
		} else {
			vec![rotation_broadcast_id]
		}
	}
}

// This is to be used both for ingress/egress estimation and for setting the compute units
// limit when crafting transactions by the State Chain.
pub mod compute_units_costs {
	use super::{SolAmount, SolComputeLimit};

	// Applying a 50% buffer to ensure we'll have enough compute units to cover the actual cost.
	pub const fn compute_limit_with_buffer(
		compute_limit_value: SolComputeLimit,
	) -> SolComputeLimit {
		compute_limit_value * 3 / 2
	}

	pub const BASE_COMPUTE_UNITS_PER_TX: SolComputeLimit = 450u32;
	pub const COMPUTE_UNITS_PER_FETCH_NATIVE: SolComputeLimit = 25_000u32;
	pub const COMPUTE_UNITS_PER_TRANSFER_NATIVE: SolComputeLimit = 150u32;
	pub const COMPUTE_UNITS_PER_FETCH_TOKEN: SolComputeLimit = 45_000u32;
	pub const COMPUTE_UNITS_PER_TRANSFER_TOKEN: SolComputeLimit = 50_000u32;
	pub const COMPUTE_UNITS_PER_ROTATION: SolComputeLimit = 8_000u32;
	pub const COMPUTE_UNITS_PER_SET_GOV_KEY: SolComputeLimit = 15_000u32;
	pub const COMPUTE_UNITS_PER_BUMP_DERIVATION: SolComputeLimit = 2_000u32;
	pub const COMPUTE_UNITS_PER_FETCH_AND_CLOSE_VAULT_SWAP_ACCOUNTS: SolComputeLimit = 20_000u32;
	pub const COMPUTE_UNITS_PER_CLOSE_ACCOUNT: SolComputeLimit = 6_000u32;
	pub const COMPUTE_UNITS_PER_SET_PROGRAM_SWAPS_PARAMS: SolComputeLimit = 50_000u32;
	pub const COMPUTE_UNITS_PER_ENABLE_TOKEN_SUPPORT: SolComputeLimit = 50_000u32;

	/// This is equivalent to a priority fee, in micro-lamports/compute unit.
	pub const MIN_COMPUTE_PRICE: SolAmount = 10_000_000;

	// Max compute units per CCM transfers. Capping it to maximize chances of inclusion.
	pub const MAX_COMPUTE_UNITS_PER_CCM_TRANSFER: SolComputeLimit = 600_000u32;
	// Compute units overhead for Ccm transfers. These also act as minimum compute units
	// to ensure transaction inclusion.
	pub const CCM_COMPUTE_UNITS_OVERHEAD_NATIVE: SolComputeLimit = 40_000u32;
	pub const CCM_COMPUTE_UNITS_OVERHEAD_TOKEN: SolComputeLimit = 80_000u32;
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

impl SolTrackedData {
	pub fn calculate_ccm_compute_limit(
		gas_budget: cf_primitives::GasAmount,
		asset: SolAsset,
	) -> SolComputeLimit {
		use compute_units_costs::*;

		let compute_limit: SolComputeLimit = match gas_budget.try_into() {
			Ok(limit) => limit,
			Err(_) => return MAX_COMPUTE_UNITS_PER_CCM_TRANSFER,
		};
		let compute_limit_with_overhead = compute_limit.saturating_add(match asset {
			SolAsset::Sol => CCM_COMPUTE_UNITS_OVERHEAD_NATIVE,
			SolAsset::SolUsdc => CCM_COMPUTE_UNITS_OVERHEAD_TOKEN,
		});
		sp_std::cmp::min(MAX_COMPUTE_UNITS_PER_CCM_TRANSFER, compute_limit_with_overhead)
	}

	// Calculate the estimated fee for broadcasting a transaction given its compute units
	// and the current priority fee.
	pub fn calculate_transaction_fee(
		&self,
		compute_units: SolComputeLimit,
	) -> <Solana as crate::Chain>::ChainAmount {
		use compute_units_costs::*;

		// Match the minimum compute price that will be set on broadcast.
		let priority_fee = sp_std::cmp::max(self.priority_fee, MIN_COMPUTE_PRICE);

		LAMPORTS_PER_SIGNATURE.saturating_add(
			// It should never approach overflow but just in case
			sp_std::cmp::min(
				SolAmount::MAX as u128,
				(priority_fee as u128 * compute_units as u128)
					.div_ceil(MICROLAMPORTS_PER_LAMPORT.into()),
			) as SolAmount,
		)
	}
}

impl FeeEstimationApi<Solana> for SolTrackedData {
	fn estimate_egress_fee(
		&self,
		asset: <Solana as crate::Chain>::ChainAsset,
	) -> <Solana as crate::Chain>::ChainAmount {
		use compute_units_costs::*;

		let compute_units_per_transfer = compute_limit_with_buffer(
			BASE_COMPUTE_UNITS_PER_TX +
				match asset {
					assets::sol::Asset::Sol => COMPUTE_UNITS_PER_TRANSFER_NATIVE,
					assets::sol::Asset::SolUsdc => COMPUTE_UNITS_PER_TRANSFER_TOKEN,
				},
		);

		let gas_fee = self.calculate_transaction_fee(compute_units_per_transfer);

		match asset {
			assets::sol::Asset::Sol => gas_fee,
			assets::sol::Asset::SolUsdc => gas_fee.saturating_add(TOKEN_ACCOUNT_RENT),
		}
	}
	fn estimate_ingress_fee(
		&self,
		asset: <Solana as crate::Chain>::ChainAsset,
	) -> <Solana as crate::Chain>::ChainAmount {
		use compute_units_costs::*;

		let compute_units_per_fetch = compute_limit_with_buffer(
			BASE_COMPUTE_UNITS_PER_TX +
				match asset {
					assets::sol::Asset::Sol => COMPUTE_UNITS_PER_FETCH_NATIVE,
					assets::sol::Asset::SolUsdc => COMPUTE_UNITS_PER_FETCH_TOKEN,
				},
		);

		self.calculate_transaction_fee(compute_units_per_fetch)
	}

	fn estimate_ingress_fee_vault_swap(&self) -> Option<<Solana as Chain>::ChainAmount> {
		use compute_units_costs::*;

		// Some of the fetches might be batches but we need to estimate pessimistically.
		let compute_units_per_fetch_and_close = compute_limit_with_buffer(
			COMPUTE_UNITS_PER_FETCH_AND_CLOSE_VAULT_SWAP_ACCOUNTS + COMPUTE_UNITS_PER_CLOSE_ACCOUNT,
		);

		Some(self.calculate_transaction_fee(compute_units_per_fetch_and_close))
	}

	fn estimate_ccm_fee(
		&self,
		asset: <Solana as Chain>::ChainAsset,
		gas_budget: cf_primitives::GasAmount,
		_message_length: usize,
	) -> Option<<Solana as Chain>::ChainAmount> {
		let gas_limit = SolTrackedData::calculate_ccm_compute_limit(gas_budget, asset);
		let ccm_fee = self.calculate_transaction_fee(gas_limit);
		Some(match asset {
			assets::sol::Asset::Sol => ccm_fee,
			assets::sol::Asset::SolUsdc => ccm_fee.saturating_add(TOKEN_ACCOUNT_RENT),
		})
	}
}

impl FeeRefundCalculator<Solana> for SolanaTransactionData {
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

		/// Utility for printing out pub and private keys for a keypair. Used to easily save
		/// generate keypair.
		pub fn print_pub_and_private_keys(&self) {
			println!(
				"Pubkey: {:?} \nhex: {:?} \nraw bytes: {:?}",
				cf_utilities::bs58_string(self.pubkey().0),
				hex::encode(self.to_bytes()),
				self.to_bytes()
			);
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

/// Solana Environment variables used when building the base API call.
#[derive(
	Encode, Decode, TypeInfo, Default, Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize,
)]
pub struct SolApiEnvironment {
	// For native Sol API calls.
	pub vault_program: SolAddress,
	pub vault_program_data_account: SolAddress,

	// For token API calls.
	pub token_vault_pda_account: SolAddress,

	// For Usdc token
	pub usdc_token_mint_pubkey: SolAddress,
	pub usdc_token_vault_ata: SolAddress,

	// For program swaps API calls.
	pub swap_endpoint_program: SolAddress,
	pub swap_endpoint_program_data_account: SolAddress,
}

impl DepositDetailsToTransactionInId<SolanaCrypto> for () {}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{sol::compute_units_costs::*, ChannelLifecycleHooks};

	#[test]
	fn can_calculate_gas_limit() {
		const TEST_EGRESS_BUDGET: u128 = 80_000u128;

		for asset in &[SolAsset::Sol, SolAsset::SolUsdc] {
			let default_compute_limit = match asset {
				SolAsset::Sol => CCM_COMPUTE_UNITS_OVERHEAD_NATIVE,
				SolAsset::SolUsdc => CCM_COMPUTE_UNITS_OVERHEAD_TOKEN,
			};

			let mut tx_compute_limit: u32 =
				SolTrackedData::calculate_ccm_compute_limit(TEST_EGRESS_BUDGET, *asset);
			assert_eq!(tx_compute_limit, TEST_EGRESS_BUDGET as u32 + default_compute_limit);

			// Test SolComputeLimit saturation
			assert_eq!(
				SolTrackedData::calculate_ccm_compute_limit(
					MAX_COMPUTE_UNITS_PER_CCM_TRANSFER as u128 - default_compute_limit as u128 + 1,
					*asset,
				),
				MAX_COMPUTE_UNITS_PER_CCM_TRANSFER
			);

			// Test upper cap
			tx_compute_limit = SolTrackedData::calculate_ccm_compute_limit(
				MAX_COMPUTE_UNITS_PER_CCM_TRANSFER as u128 - 1,
				*asset,
			);
			assert_eq!(tx_compute_limit, MAX_COMPUTE_UNITS_PER_CCM_TRANSFER);

			// Test lower cap
			let tx_compute_limit = SolTrackedData::calculate_ccm_compute_limit(0, *asset);
			assert_eq!(tx_compute_limit, default_compute_limit);
		}
	}

	#[test]
	fn solana_channel_recycling_is_assumed_to_be_deactivated() {
		assert!(
			<<Solana as Chain>::DepositChannelState as ChannelLifecycleHooks>::maybe_recycle(0).is_none(),
			"It looks like Solana channel recycling is active. If this is intentional, ensure that the corresponding
			unsynchronised state map in the delta_based_ingress election is not deleted when channels are closed."
		);
	}
}
