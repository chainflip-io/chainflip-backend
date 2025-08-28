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

pub use cf_primitives::chains::Solana;

use cf_primitives::{
	AffiliateAndFee, BasisPoints, Beneficiary, ChannelId, DcaParameters, ForeignChain,
};
use sol_prim::program_instructions::FunctionDiscriminator;
use sp_core::ConstBool;
use sp_std::{vec, vec::Vec};

use crate::{
	address::{self, EncodedAddress},
	assets,
	cf_parameters::VaultSwapParametersV1,
	sol::sol_tx_core::{
		instructions::program_instructions::swap_endpoints::{
			SwapNativeParams, SwapTokenParams, XSwapNative, XSwapToken,
		},
		AccountBump, SlotNumber,
	},
	AnyChainAsset, CcmAdditionalData, CcmChannelMetadata, CcmChannelMetadataUnchecked, CcmParams,
	ChannelRefundParametersUncheckedEncoded, DepositChannel, DepositDetailsToTransactionInId,
	FeeEstimationApi, FeeRefundCalculator, TypeInfo,
};
use codec::{Decode, Encode, FullCodec, MaxEncodedLen};
use frame_support::{
	sp_runtime::{BoundedVec, RuntimeDebug},
	Parameter,
};
use serde::{Deserialize, Serialize};
use sp_runtime::{helpers_128bit::multiply_by_rational_with_rounding, traits::Member};

use super::{Chain, ChainCrypto};

pub mod api;
pub mod benchmarking;
pub mod instruction_builder;
pub mod sol_tx_core;
mod tests;
pub mod transaction_builder;

pub use crate::assets::sol::Asset as SolAsset;
use crate::benchmarking_value::BenchmarkValue;
pub use sol_prim::{
	consts::{
		LAMPORTS_PER_SIGNATURE, MAX_TRANSACTION_LENGTH, MICROLAMPORTS_PER_LAMPORT,
		SOLANA_PDA_MAX_SEED_LEN, TOKEN_ACCOUNT_RENT,
	},
	pda::{Pda as DerivedAddressBuilder, PdaError as AddressDerivationError},
	transaction::{
		v0::VersionedMessageV0 as SolVersionedMessageV0, VersionedMessage as SolVersionedMessage,
		VersionedTransaction as SolVersionedTransaction,
	},
	Address as SolAddress, AddressLookupTableAccount as SolAddressLookupTableAccount,
	AddressLookupTableAccount, Amount as SolAmount, ComputeLimit as SolComputeLimit,
	Digest as SolHash, Hash as RawSolHash, Instruction as SolInstruction,
	InstructionRpc as SolInstructionRpc, Pubkey as SolPubkey, Signature as SolSignature,
	SlotNumber as SolBlockNumber,
};
pub use sol_tx_core::{
	rpc_types, AccountMeta as SolAccountMeta, CcmAccounts as SolCcmAccounts,
	CcmAddress as SolCcmAddress,
};

// Due to transaction size limit in Solana, we have a limit on number of fetches in a solana fetch
// tx. Batches of 5 fetches get to ~1000 bytes, max ~1090 for tokens.
pub const MAX_SOL_FETCHES_PER_TX: usize = 5;

// Bytes left that are available for the user when building the native and token ccm transfers.
// All function parameters are already accounted except additional_accounts and message.
pub const MAX_USER_CCM_BYTES_SOL: usize = MAX_TRANSACTION_LENGTH - 418usize; // 814 bytes left
pub const MAX_USER_CCM_BYTES_USDC: usize = MAX_TRANSACTION_LENGTH - 507usize; // 725 bytes left

/// Maximum number of Accounts Lookup Tables user can pass in as part of CCM call.
pub const MAX_CCM_USER_ALTS: u8 = 3u8;

// Nonce management values
pub const NONCE_NUMBER_CRITICAL_NONCES: usize = 1;
pub const NONCE_AVAILABILITY_THRESHOLD_FOR_INITIATING_TRANSFER: usize = 1;

// Values used when closing vault swap accounts.
pub const MAX_BATCH_SIZE_OF_VAULT_SWAP_ACCOUNT_CLOSURES: usize = 5;
pub const MAX_WAIT_BLOCKS_FOR_SWAP_ACCOUNT_CLOSURE_APICALLS: u32 = 14400;
pub const NONCE_AVAILABILITY_THRESHOLD_FOR_INITIATING_SWAP_ACCOUNT_CLOSURES: usize = 3;

pub const REFERENCE_SOL_PRICE_IN_USD: u128 = 145_000_000u128; //145 usd

// Use serialized transaction
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub struct SolanaTransactionData {
	pub serialized_transaction: Vec<u8>,
	pub skip_preflight: bool,
}

/// A Solana transaction in id is a tuple of the AccountAddress and the slot number.
pub type SolanaTransactionInId = (SolAddress, u64);

pub type SolSeed = BoundedVec<u8, sp_core::ConstU32<SOLANA_PDA_MAX_SEED_LEN>>;

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

	fn input_asset_amount_using_reference_gas_asset_price(
		input_asset: Self::ChainAsset,
		required_gas: Self::ChainAmount,
	) -> Self::ChainAmount {
		match input_asset {
			assets::sol::Asset::Sol => required_gas,
			assets::sol::Asset::SolUsdc => multiply_by_rational_with_rounding(
				required_gas.into(),
				REFERENCE_SOL_PRICE_IN_USD,
				1_000_000_000u128,
				sp_runtime::Rounding::Up,
			)
			.map(|v| v.try_into().unwrap_or(0u64))
			.unwrap_or(0u64),
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SolanaCrypto;

impl ChainCrypto for SolanaCrypto {
	const NAME: &'static str = "Solana";
	type UtxoChain = ConstBool<false>;
	type KeyHandoverIsRequired = ConstBool<false>;

	type AggKey = SolAddress;
	type Signer = SolAddress;
	type Signature = SolSignature;
	type Payload = SolVersionedMessage;
	type ThresholdSignature = SolSignature;
	type TransactionInId = SolanaTransactionInId;
	type TransactionOutId = Self::ThresholdSignature;

	type GovKey = SolAddress;

	fn verify_signature(
		signer: &Self::Signer,
		payload: &[u8],
		signature: &Self::Signature,
	) -> bool {
		use sp_core::ed25519::{Public, Signature};
		use sp_io::crypto::ed25519_verify;

		ed25519_verify(&Signature::from_raw(signature.0), payload, &Public::from_raw(signer.0))
	}

	fn verify_threshold_signature(
		agg_key: &Self::AggKey,
		payload: &Self::Payload,
		signature: &Self::ThresholdSignature,
	) -> bool {
		Self::verify_signature(agg_key, payload.serialize().as_slice(), signature)
	}

	fn agg_key_to_payload(agg_key: Self::AggKey, _for_handover: bool) -> Self::Payload {
		SolVersionedMessage::new(&[], Some(SolPubkey::from(agg_key)), None, &[])
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
	pub const COMPUTE_UNITS_PER_ROTATION: SolComputeLimit = 5_000u32;
	pub const COMPUTE_UNITS_PER_NONCE_ROTATION: SolComputeLimit = 4_000u32;
	pub const COMPUTE_UNITS_PER_SET_GOV_KEY: SolComputeLimit = 15_000u32;
	pub const COMPUTE_UNITS_PER_BUMP_DERIVATION: SolComputeLimit = 2_000u32;
	pub const COMPUTE_UNITS_PER_FETCH_AND_CLOSE_VAULT_SWAP_ACCOUNTS: SolComputeLimit = 20_000u32;
	pub const COMPUTE_UNITS_PER_CLOSE_ACCOUNT: SolComputeLimit = 6_000u32;
	pub const COMPUTE_UNITS_PER_SET_PROGRAM_SWAPS_PARAMS: SolComputeLimit = 50_000u32;
	pub const COMPUTE_UNITS_PER_ENABLE_TOKEN_SUPPORT: SolComputeLimit = 50_000u32;
	pub const COMPUTE_UNITS_PER_UPGRADE_PROGRAM: SolComputeLimit = 100_000u32;

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
	Encode, Decode, TypeInfo, Default, Clone, PartialEq, Eq, Debug, Serialize, Deserialize,
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
	pub alt_manager_program: SolAddress,
	pub address_lookup_table_account: AddressLookupTableAccount,
}

impl DepositDetailsToTransactionInId<SolanaCrypto> for () {}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct DecodedXSwapParams {
	pub amount: cf_primitives::AssetAmount,
	pub src_asset: AnyChainAsset,
	pub src_address: SolAddress,
	pub from_token_account: Option<SolAddress>,
	pub dst_address: crate::address::EncodedAddress,
	pub dst_token: AnyChainAsset,
	pub refund_parameters: ChannelRefundParametersUncheckedEncoded,
	pub dca_parameters: Option<DcaParameters>,
	pub boost_fee: u8,
	pub broker_id: cf_primitives::AccountId,
	pub broker_commission: BasisPoints,
	pub affiliate_fees: Vec<AffiliateAndFee>,
	pub ccm: Option<CcmChannelMetadataUnchecked>,
	pub seed: SolSeed,
}

pub fn decode_sol_instruction_data(
	instruction: &SolInstruction,
) -> Result<DecodedXSwapParams, &'static str> {
	let data = instruction.data.clone();
	let (
		amount,
		src_asset,
		src_token_from_account,
		dst_chain,
		dst_address,
		dst_token,
		ccm_parameters,
		cf_parameters,
		seed,
	) = match instruction.accounts.len() as u8 {
		sol_tx_core::consts::X_SWAP_NATIVE_ACC_LEN => {
			let (
				_discriminator,
				XSwapNative {
					swap_native_params:
						SwapNativeParams {
							amount,
							dst_chain,
							dst_address,
							dst_token,
							ccm_parameters,
							cf_parameters,
						},
					seed,
				},
			) = SolInstruction::deserialize_data_with_borsh::<(FunctionDiscriminator, XSwapNative)>(
				data,
			)
			.map_err(|_| "Failed to deserialize SolInstruction")?;
			Ok((
				amount,
				AnyChainAsset::Sol,
				None,
				dst_chain,
				dst_address,
				dst_token,
				ccm_parameters,
				cf_parameters,
				seed,
			))
		},
		sol_tx_core::consts::X_SWAP_TOKEN_ACC_LEN => {
			let (
				_discriminator,
				XSwapToken {
					swap_token_params:
						SwapTokenParams {
							amount,
							dst_chain,
							dst_address,
							dst_token,
							ccm_parameters,
							cf_parameters,
							decimals: _,
						},
					seed,
				},
			) = SolInstruction::deserialize_data_with_borsh::<(FunctionDiscriminator, XSwapToken)>(
				data,
			)
			.map_err(|_| "Failed to deserialize SolInstruction")?;
			Ok((
				amount,
				AnyChainAsset::SolUsdc,
				Some(
					instruction
						.accounts
						.get(sol_tx_core::consts::X_SWAP_TOKEN_FROM_TOKEN_ACC_IDX as usize)
						.ok_or("Invalid accounts in SolInstruction")?
						.pubkey
						.into(),
				),
				dst_chain,
				dst_address,
				dst_token,
				ccm_parameters,
				cf_parameters,
				seed,
			))
		},
		_ => Err("SolInstruction is invalid"),
	}?;

	let chain = ForeignChain::try_from(dst_chain).map_err(|_| "ForeignChain is invalid")?;

	let (
		VaultSwapParametersV1 {
			refund_params,
			dca_params,
			boost_fee,
			broker_fee: Beneficiary { account, bps },
			affiliate_fees,
		},
		ccm,
	) = match ccm_parameters {
		Some(CcmParams { message, gas_amount }) => {
			let (decoded, additional_data) = crate::cf_parameters::decode_cf_parameters::<
				SolAddress,
				CcmAdditionalData,
			>(&cf_parameters[..])?;
			(
				decoded,
				Some(CcmChannelMetadata {
					message: message.try_into().map_err(|_| "Ccm message is too long")?,
					gas_budget: gas_amount.into(),
					ccm_additional_data: additional_data,
				}),
			)
		},
		None => (
			crate::cf_parameters::decode_cf_parameters::<SolAddress, ()>(&cf_parameters[..])?.0,
			None,
		),
	};

	Ok(DecodedXSwapParams {
		amount: amount.into(),
		src_asset,
		src_address: instruction
			.accounts
			.get(sol_tx_core::consts::X_SWAP_FROM_ACC_IDX as usize)
			.ok_or("Invalid accounts in SolInstruction")?
			.pubkey
			.into(),
		from_token_account: src_token_from_account,
		dst_address: EncodedAddress::from_chain_bytes(chain, dst_address)?,
		dst_token: AnyChainAsset::try_from(dst_token).map_err(|_| "Invalid dst_token")?,
		refund_parameters: refund_params.map_address(Into::into),
		dca_parameters: dca_params,
		boost_fee,
		broker_id: account,
		broker_commission: bps,
		affiliate_fees: affiliate_fees.to_vec(),
		ccm,
		seed: seed.try_into().map_err(|_| "Seed too long")?,
	})
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		cf_parameters::build_and_encode_cf_parameters,
		sol::{
			compute_units_costs::*,
			instruction_builder::SolanaInstructionBuilder,
			sol_tx_core::{
				address_derivation::{
					derive_swap_endpoint_native_vault_account, derive_vault_swap_account,
				},
				sol_test_values,
			},
		},
		ChannelLifecycleHooks, ChannelRefundParametersForChain,
	};
	use cf_primitives::{chains::assets::any::Asset, AffiliateShortId};
	use sp_runtime::AccountId32;

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

	#[test]
	fn can_decode_v0_refund_params() {
		let swap_endpoint_native_vault =
			derive_swap_endpoint_native_vault_account(sol_test_values::SWAP_ENDPOINT_PROGRAM)
				.unwrap()
				.address;
		let destination_asset = Asset::Sol;
		let destination_address = EncodedAddress::Sol([0xF0; 32]);
		let from = SolPubkey([0xF1; 32]);
		let seed: &[u8] = &[0xF2; 32];
		let event_data_account =
			derive_vault_swap_account(sol_test_values::SWAP_ENDPOINT_PROGRAM, from.into(), seed)
				.unwrap()
				.address;
		let input_amount = 1_000_000_000u64;
		let refund_parameters = crate::ChannelRefundParametersV0::<<Solana as Chain>::ChainAccount> {
			retry_duration: 15u32,
			refund_address: SolAddress([0xF3; 32]),
			min_price: 0.into(),
		};
		let refund_params_v1 = ChannelRefundParametersForChain::<Solana> {
			retry_duration: refund_parameters.retry_duration,
			refund_address: refund_parameters.refund_address,
			min_price: refund_parameters.min_price,
			refund_ccm_metadata: None,
			max_oracle_price_slippage: None,
		};
		let dca_parameters = DcaParameters { number_of_chunks: 10u32, chunk_interval: 10u32 };
		let boost_fee = 10u8;
		let broker_id = AccountId32::new([0xF4; 32]);
		let broker_commission = 11;
		let affiliate_fees = vec![AffiliateAndFee { affiliate: AffiliateShortId(0u8), fee: 12u8 }];
		let channel_metadata = sol_test_values::ccm_parameter_v1().channel_metadata;

		let instruction = SolanaInstructionBuilder::x_swap_native(
			sol_test_values::api_env(),
			swap_endpoint_native_vault.into(),
			destination_asset,
			destination_address.clone(),
			from,
			seed.to_vec().try_into().unwrap(),
			event_data_account.into(),
			input_amount,
			crate::cf_parameters::build_and_encode_v0_cf_parameters(
				refund_parameters.clone(),
				Some(dca_parameters.clone()),
				boost_fee,
				broker_id.clone(),
				broker_commission,
				affiliate_fees.clone().try_into().unwrap(),
				Some(&channel_metadata),
			),
			Some(channel_metadata.clone()),
		);

		assert_eq!(
			decode_sol_instruction_data(&instruction),
			Ok(DecodedXSwapParams {
				amount: input_amount.into(),
				src_asset: Asset::Sol,
				src_address: from.into(),
				from_token_account: None,
				dst_address: destination_address,
				dst_token: destination_asset,
				refund_parameters: refund_params_v1.map_address(Into::into),
				dca_parameters: Some(dca_parameters),
				boost_fee,
				broker_id,
				broker_commission,
				affiliate_fees,
				ccm: Some(sol_test_values::ccm_metadata_v1_unchecked()),
				seed: seed.to_vec().try_into().unwrap(),
			})
		);
	}

	#[test]
	fn can_decode_x_swap_native_sol_instruction() {
		let swap_endpoint_native_vault =
			derive_swap_endpoint_native_vault_account(sol_test_values::SWAP_ENDPOINT_PROGRAM)
				.unwrap()
				.address;
		let destination_asset = Asset::Sol;
		let destination_address = EncodedAddress::Sol([0xF0; 32]);
		let from = SolPubkey([0xF1; 32]);
		let seed: &[u8] = &[0xF2; 32];
		let event_data_account =
			derive_vault_swap_account(sol_test_values::SWAP_ENDPOINT_PROGRAM, from.into(), seed)
				.unwrap()
				.address;
		let input_amount = 1_000_000_000u64;
		let refund_parameters = ChannelRefundParametersForChain::<Solana> {
			retry_duration: 15u32,
			refund_address: SolAddress([0xF3; 32]),
			min_price: 0.into(),
			refund_ccm_metadata: None,
			max_oracle_price_slippage: None,
		};
		let dca_parameters = DcaParameters { number_of_chunks: 10u32, chunk_interval: 10u32 };
		let boost_fee = 10u8;
		let broker_id = AccountId32::new([0xF4; 32]);
		let broker_commission = 11;
		let affiliate_fees = vec![AffiliateAndFee { affiliate: AffiliateShortId(0u8), fee: 12u8 }];
		let channel_metadata = sol_test_values::ccm_parameter_v1().channel_metadata;

		let instruction = SolanaInstructionBuilder::x_swap_native(
			sol_test_values::api_env(),
			swap_endpoint_native_vault.into(),
			destination_asset,
			destination_address.clone(),
			from,
			seed.to_vec().try_into().unwrap(),
			event_data_account.into(),
			input_amount,
			build_and_encode_cf_parameters(
				refund_parameters.clone(),
				Some(dca_parameters.clone()),
				boost_fee,
				broker_id.clone(),
				broker_commission,
				affiliate_fees.clone().try_into().unwrap(),
				Some(&channel_metadata),
			),
			Some(channel_metadata.clone()),
		);

		assert_eq!(
			decode_sol_instruction_data(&instruction),
			Ok(DecodedXSwapParams {
				amount: input_amount.into(),
				src_asset: Asset::Sol,
				src_address: from.into(),
				from_token_account: None,
				dst_address: destination_address,
				dst_token: destination_asset,
				refund_parameters: refund_parameters.map_address(Into::into),
				dca_parameters: Some(dca_parameters),
				boost_fee,
				broker_id,
				broker_commission,
				affiliate_fees,
				ccm: Some(sol_test_values::ccm_metadata_v1_unchecked()),
				seed: seed.to_vec().try_into().unwrap(),
			})
		);
	}

	#[test]
	fn can_decode_x_swap_usdc_sol_instruction() {
		let destination_asset = Asset::Sol;
		let destination_address = EncodedAddress::Sol([0xF0; 32]);
		let from = SolPubkey([0xF1; 32]);
		let from_token_account = SolPubkey([0xF4; 32]);
		let seed: &[u8] = &[0xF2; 32];
		let event_data_account =
			derive_vault_swap_account(sol_test_values::SWAP_ENDPOINT_PROGRAM, from.into(), seed)
				.unwrap()
				.address;
		let token_supported_account = SolPubkey([0xF5; 32]);
		let input_amount = 1_000_000_000u64;
		let refund_parameters = ChannelRefundParametersForChain::<Solana> {
			retry_duration: 15u32,
			refund_address: SolAddress([0xF3; 32]),
			min_price: 0.into(),
			refund_ccm_metadata: None,
			max_oracle_price_slippage: None,
		};
		let dca_parameters = DcaParameters { number_of_chunks: 10u32, chunk_interval: 10u32 };
		let boost_fee = 10u8;
		let broker_id = AccountId32::new([0xF4; 32]);
		let broker_commission = 11;
		let affiliate_fees = vec![AffiliateAndFee { affiliate: AffiliateShortId(0u8), fee: 12u8 }];
		let channel_metadata = sol_test_values::ccm_parameter_v1().channel_metadata;

		let instruction = SolanaInstructionBuilder::x_swap_usdc(
			sol_test_values::api_env(),
			destination_asset,
			destination_address.clone(),
			from,
			from_token_account,
			seed.to_vec().try_into().unwrap(),
			event_data_account.into(),
			token_supported_account,
			input_amount,
			build_and_encode_cf_parameters(
				refund_parameters.clone(),
				Some(dca_parameters.clone()),
				boost_fee,
				broker_id.clone(),
				broker_commission,
				affiliate_fees.clone().try_into().unwrap(),
				Some(&channel_metadata),
			),
			Some(channel_metadata.clone()),
		);

		assert_eq!(
			decode_sol_instruction_data(&instruction),
			Ok(DecodedXSwapParams {
				amount: input_amount.into(),
				src_asset: Asset::SolUsdc,
				src_address: from.into(),
				from_token_account: Some(from_token_account.into()),
				dst_address: destination_address,
				dst_token: destination_asset,
				refund_parameters: refund_parameters.map_address(Into::into),
				dca_parameters: Some(dca_parameters),
				boost_fee,
				broker_id,
				broker_commission,
				affiliate_fees,
				ccm: Some(sol_test_values::ccm_metadata_v1_unchecked()),
				seed: seed.to_vec().try_into().unwrap(),
			})
		);
	}
}
