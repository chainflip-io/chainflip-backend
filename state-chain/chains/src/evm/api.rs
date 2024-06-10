use crate::{
	eth::Address as EvmAddress,
	evm::{EvmCrypto, SchnorrVerificationComponents},
	*,
};
use common::*;
use ethabi::{Address, ParamType, Token, Uint};
use frame_support::{
	sp_runtime::traits::{Hash, Keccak256},
	traits::Defensive,
};

use super::{tokenizable::Tokenizable, EvmFetchId};

pub mod all_batch;
pub mod common;
pub mod execute_x_swap_and_call;
pub mod set_agg_key_with_agg_key;
pub mod set_comm_key_with_agg_key;
pub mod set_gov_key_with_agg_key;
pub mod transfer_fallback;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Default)]
pub struct EvmReplayProtection {
	pub nonce: u64,
	pub chain_id: EvmChainId,
	pub key_manager_address: Address,
	pub contract_address: Address,
}

impl Tokenizable for EvmReplayProtection {
	fn tokenize(self) -> Token {
		Token::FixedArray(vec![
			Token::Uint(Uint::from(self.nonce)),
			Token::Address(self.contract_address),
			Token::Uint(Uint::from(self.chain_id)),
			Token::Address(self.key_manager_address),
		])
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Tuple(vec![
			ParamType::Uint(256),
			ParamType::Address,
			ParamType::Uint(256),
			ParamType::Address,
		])
	}
}

/// The `SigData` struct used for threshold signatures in the smart contracts.
/// See [here](https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/interfaces/IShared.sol).
#[derive(
	Encode,
	Decode,
	TypeInfo,
	Copy,
	Clone,
	RuntimeDebug,
	PartialEq,
	Eq,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
pub struct SigData {
	/// The Schnorr signature.
	pub sig: Uint,
	/// The nonce value for the AggKey. Each Signature over an AggKey should have a unique
	/// nonce to prevent replay attacks.
	pub nonce: Uint,
	/// The address value derived from the random nonce value `k`. Also known as
	/// `nonceTimesGeneratorAddress`.
	///
	/// Note this is unrelated to the `nonce` above. The nonce in the context of
	/// `nonceTimesGeneratorAddress` is a generated as part of each signing round (ie. as part
	/// of the Schnorr signature) to prevent certain classes of cryptographic attacks.
	pub k_times_g_address: Address,
}

impl SigData {
	/// Add the actual signature. This method does no verification.
	pub fn new(nonce: impl Into<Uint>, schnorr: &SchnorrVerificationComponents) -> Self {
		Self {
			sig: schnorr.s.into(),
			nonce: nonce.into(),
			k_times_g_address: schnorr.k_times_g_address.into(),
		}
	}
}

impl Tokenizable for SigData {
	fn tokenize(self) -> Token {
		Token::Tuple(vec![
			self.sig.tokenize(),
			self.nonce.tokenize(),
			self.k_times_g_address.tokenize(),
		])
	}

	fn param_type() -> ParamType {
		ParamType::Tuple(vec![ParamType::Uint(256), ParamType::Uint(256), ParamType::Address])
	}
}

pub trait EvmCall {
	const FUNCTION_NAME: &'static str;

	/// The function names and parameters, not including sigData.
	fn function_params() -> Vec<(&'static str, ethabi::ParamType)>;
	/// The function values to be used as call parameters, no including sigData.
	fn function_call_args(&self) -> Vec<Token>;

	fn get_function() -> ethabi::Function {
		#[allow(deprecated)]
		ethabi::Function {
			name: Self::FUNCTION_NAME.into(),
			inputs: core::iter::once(("sigData", SigData::param_type()))
				.chain(Self::function_params())
				.map(|(n, t)| ethabi_param(n, t))
				.collect(),
			outputs: vec![],
			constant: None,
			state_mutability: ethabi::StateMutability::NonPayable,
		}
	}
	/// Encodes the call and signature into EVM Abi format.
	fn abi_encoded(&self, sig_data: &SigData) -> Vec<u8> {
		Self::get_function()
			.encode_input(
				&core::iter::once(sig_data.tokenize())
					.chain(self.function_call_args())
					.collect::<Vec<_>>(),
			)
			.expect(
				r#"
					This can only fail if the parameter types don't match the function signature.
					Therefore, as long as the tests pass, it can't fail at runtime.
				"#,
			)
	}
	/// Generates the message hash for this call.
	fn msg_hash(&self) -> <Keccak256 as Hash>::Output {
		Keccak256::hash(&ethabi::encode(
			&core::iter::once(Self::get_function().tokenize())
				.chain(self.function_call_args())
				.collect::<Vec<_>>(),
		))
	}
	fn gas_budget(&self) -> Option<<Ethereum as Chain>::ChainAmount> {
		None
	}
}

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct EvmTransactionBuilder<C> {
	pub sig_data: Option<SigData>,
	pub replay_protection: EvmReplayProtection,
	pub call: C,
}

impl<C: EvmCall> EvmTransactionBuilder<C> {
	pub fn new_unsigned(replay_protection: EvmReplayProtection, call: C) -> Self {
		Self { replay_protection, call, sig_data: None }
	}

	pub fn replay_protection(&self) -> EvmReplayProtection {
		self.replay_protection
	}

	pub fn chain_id(&self) -> EvmChainId {
		self.replay_protection.chain_id
	}

	pub fn gas_budget(&self) -> Option<<Ethereum as Chain>::ChainAmount> {
		self.call.gas_budget()
	}

	pub fn threshold_signature_payload(&self) -> <EvmCrypto as ChainCrypto>::Payload {
		Keccak256::hash(&ethabi::encode(&[
			self.call.msg_hash().tokenize(),
			self.replay_protection.tokenize(),
		]))
	}

	fn expect_sig_data_defensive(&self, reason: &'static str) -> SigData {
		self.sig_data.defensive_proof(reason).unwrap_or(SigData {
			sig: Default::default(),
			nonce: Default::default(),
			k_times_g_address: Default::default(),
		})
	}

	pub fn signed(
		mut self,
		threshold_signature: &<EvmCrypto as ChainCrypto>::ThresholdSignature,
	) -> Self {
		self.sig_data = Some(SigData::new(self.replay_protection.nonce, threshold_signature));
		self
	}

	pub fn chain_encoded(&self) -> Vec<u8> {
		self.call.abi_encoded(
			&self.expect_sig_data_defensive("`chain_encoded` is only called on signed api calls."),
		)
	}

	pub fn is_signed(&self) -> bool {
		self.sig_data.is_some()
	}

	pub fn transaction_out_id(&self) -> <EvmCrypto as ChainCrypto>::TransactionOutId {
		let sig_data = self.expect_sig_data_defensive(
			"`transaction_out_id` is only requested for signed transactions.",
		);
		SchnorrVerificationComponents {
			s: sig_data.sig.into(),
			k_times_g_address: sig_data.k_times_g_address.into(),
		}
	}

	pub fn refresh_replay_protection(&mut self, replay_protection: EvmReplayProtection) {
		self.sig_data = None;
		self.replay_protection = replay_protection;
	}
}

pub type EvmChainId = u64;

pub(super) fn ethabi_param(name: &'static str, param_type: ethabi::ParamType) -> ethabi::Param {
	ethabi::Param { name: name.into(), kind: param_type, internal_type: None }
}

pub fn evm_all_batch_builder<
	C: Chain<DepositFetchId = EvmFetchId, ChainAccount = EvmAddress, ChainAmount = u128>,
	F: Fn(<C as Chain>::ChainAsset) -> Option<EvmAddress>,
>(
	fetch_params: Vec<FetchAssetParams<C>>,
	transfer_params: Vec<TransferAssetParams<C>>,
	token_address_fn: F,
	replay_protection: EvmReplayProtection,
) -> Result<EvmTransactionBuilder<all_batch::AllBatch>, AllBatchError> {
	let mut fetch_only_params = vec![];
	let mut fetch_deploy_params = vec![];
	for FetchAssetParams { deposit_fetch_id, asset } in fetch_params {
		if let Some(token_address) = token_address_fn(asset) {
			match deposit_fetch_id {
				EvmFetchId::Fetch(contract_address) => {
					debug_assert!(
						asset != <C as Chain>::GAS_ASSET,
						"Eth should not be fetched. It is auto-fetched in the smart contract."
					);
					fetch_only_params
						.push(EncodableFetchAssetParams { contract_address, asset: token_address })
				},
				EvmFetchId::DeployAndFetch(channel_id) => fetch_deploy_params
					.push(EncodableFetchDeployAssetParams { channel_id, asset: token_address }),
				EvmFetchId::NotRequired => (),
			};
		} else {
			return Err(AllBatchError::UnsupportedToken)
		}
	}
	if fetch_only_params.is_empty() && fetch_deploy_params.is_empty() && transfer_params.is_empty()
	{
		Err(AllBatchError::NotRequired)
	} else {
		Ok(EvmTransactionBuilder::new_unsigned(
			replay_protection,
			all_batch::AllBatch::new(
				fetch_deploy_params,
				fetch_only_params,
				transfer_params
					.into_iter()
					.map(|TransferAssetParams { asset, to, amount }| {
						token_address_fn(asset)
							.map(|address| EncodableTransferAssetParams {
								to,
								amount,
								asset: address,
							})
							.ok_or(AllBatchError::UnsupportedToken)
					})
					.collect::<Result<Vec<_>, _>>()?,
			),
		))
	}
}

/// Provides the environment data for ethereum-like chains.
pub trait EvmEnvironmentProvider<C: Chain> {
	fn token_address(asset: <C as Chain>::ChainAsset) -> Option<EvmAddress>;
	fn key_manager_address() -> EvmAddress;
	fn vault_address() -> EvmAddress;
	fn chain_id() -> EvmChainId;
	fn next_nonce() -> u64;
}

#[cfg(test)]
mod tests {
	use super::*;
	use evm::AggKey;

	#[test]
	fn test_evm_transaction_builder() {
		let replay_protection = EvmReplayProtection {
			nonce: 1,
			chain_id: 1,
			key_manager_address: Address::from_low_u64_be(1),
			contract_address: Address::from_low_u64_be(2),
		};

		let call = set_agg_key_with_agg_key::SetAggKeyWithAggKey { new_key: AggKey::default() };
		let builder = EvmTransactionBuilder::new_unsigned(replay_protection, call);

		assert_eq!(builder.chain_id(), 1);
		assert_eq!(builder.replay_protection(), replay_protection);
		assert_eq!(builder.gas_budget(), None);
		assert!(!builder.is_signed());

		let threshold_signature =
			SchnorrVerificationComponents { s: [0u8; 32], k_times_g_address: [0u8; 20] };

		let mut builder = builder.signed(&threshold_signature);
		assert!(builder.is_signed());

		let new_replay_protection = EvmReplayProtection {
			nonce: 2,
			chain_id: 2,
			key_manager_address: Address::from_low_u64_be(3),
			contract_address: Address::from_low_u64_be(4),
		};

		builder.refresh_replay_protection(new_replay_protection);

		assert_eq!(builder.replay_protection(), new_replay_protection);
		assert!(!builder.is_signed());
	}
}
