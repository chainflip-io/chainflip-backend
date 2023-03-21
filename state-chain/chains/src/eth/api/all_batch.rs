use cf_primitives::{AssetAmount, IntentId};
use codec::{Decode, Encode};
use ethabi::{Address, ParamType, Token, Uint};
use scale_info::TypeInfo;
use sp_std::{boxed::Box, vec, vec::Vec};

use crate::{
	eth::{ingress_address::get_salt, Ethereum, SigData, Tokenizable},
	impl_api_call_eth, ApiCall, ChainCrypto,
};

use super::{ethabi_function, ethabi_param, EthereumReplayProtection};

use sp_runtime::RuntimeDebug;

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub(crate) struct EncodableFetchAssetParams {
	pub contract_address: Address,
	pub asset: Address,
}

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub(crate) struct EncodableFetchDeployAssetParams {
	pub intent_id: IntentId,
	pub asset: Address,
}

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub(crate) struct EncodableTransferAssetParams {
	/// For Ethereum, the asset is encoded as a contract address.
	pub asset: Address,
	pub to: Address,
	pub amount: AssetAmount,
}

impl Tokenizable for EncodableFetchDeployAssetParams {
	fn tokenize(self) -> Token {
		Token::Tuple(vec![
			Token::FixedBytes(get_salt(self.intent_id).to_vec()),
			Token::Address(self.asset),
		])
	}
}
impl Tokenizable for EncodableFetchAssetParams {
	fn tokenize(self) -> Token {
		Token::Tuple(vec![Token::Address(self.contract_address), Token::Address(self.asset)])
	}
}

impl<T: Tokenizable> Tokenizable for Vec<T> {
	fn tokenize(self) -> Token {
		Token::Array(self.into_iter().map(|t| t.tokenize()).collect())
	}
}

impl Tokenizable for EncodableTransferAssetParams {
	fn tokenize(self) -> Token {
		Token::Tuple(vec![
			Token::Address(self.asset),
			Token::Address(self.to),
			Token::Uint(Uint::from(self.amount)),
		])
	}
}

/// Represents all the arguments required to build the call to Vault's 'allBatch'
/// function.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub struct AllBatch {
	/// The signature data for validation and replay protection.
	sig_data: SigData,
	/// The list of all inbound deposits that are to be fetched that need to deploy new deposit
	/// contracts.
	fetch_deploy_params: Vec<EncodableFetchDeployAssetParams>,
	/// The list of all inbound deposits that are to be fetched that reuse already deployed deposit
	/// contracts.
	fetch_params: Vec<EncodableFetchAssetParams>,
	/// The list of all outbound transfers that need to be made to given addresses.
	transfer_params: Vec<EncodableTransferAssetParams>,
}

impl AllBatch {
	pub(crate) fn new_unsigned(
		replay_protection: EthereumReplayProtection,
		fetch_deploy_params: Vec<EncodableFetchDeployAssetParams>,
		fetch_params: Vec<EncodableFetchAssetParams>,
		transfer_params: Vec<EncodableTransferAssetParams>,
	) -> Self {
		let mut calldata = Self {
			sig_data: SigData::new_empty(replay_protection),
			fetch_deploy_params,
			fetch_params,
			transfer_params,
		};
		calldata.sig_data.insert_msg_hash_from(calldata.abi_encoded().as_slice());

		calldata
	}

	fn get_function(&self) -> ethabi::Function {
		ethabi_function(
			"allBatch",
			vec![
				ethabi_param(
					"sigData",
					ParamType::Tuple(vec![
						ParamType::Address,
						ParamType::Uint(256),
						ParamType::Uint(256),
						ParamType::Uint(256),
						ParamType::Uint(256),
						ParamType::Address,
					]),
				),
				ethabi_param(
					"deployFetchParamsArray",
					ParamType::Array(Box::new(ParamType::Tuple(vec![
						ParamType::FixedBytes(32),
						ParamType::Address,
					]))),
				),
				ethabi_param(
					"fetchParamsArray",
					ParamType::Array(Box::new(ParamType::Tuple(vec![
						ParamType::Address,
						ParamType::Address,
					]))),
				),
				ethabi_param(
					"transferParamsArray",
					ParamType::Array(Box::new(ParamType::Tuple(vec![
						ParamType::Address,
						ParamType::Address,
						ParamType::Uint(256),
					]))),
				),
			],
		)
	}

	fn abi_encoded(&self) -> Vec<u8> {
		self.get_function()
			.encode_input(&[
				self.sig_data.tokenize(),
				self.fetch_deploy_params.clone().tokenize(),
				self.fetch_params.clone().tokenize(),
				self.transfer_params.clone().tokenize(),
			])
			.expect(
				r#"
						This can only fail if the parameter types don't match the function signature encoded below.
						Therefore, as long as the tests pass, it can't fail at runtime.
					"#,
			)
	}
}

impl_api_call_eth!(AllBatch);

#[cfg(test)]
mod test_all_batch {
	use crate::eth::SchnorrVerificationComponents;

	use super::*;
	use frame_support::assert_ok;

	#[test]
	// There have been obtuse test failures due to the loading of the contract failing
	// It uses a different ethabi to the CFE, so we test separately
	fn just_load_the_contract() {
		assert_ok!(ethabi::Contract::load(
			std::include_bytes!("../../../../../engine/src/eth/abis/Vault.json").as_ref(),
		));
	}

	#[test]
	fn test_payload() {
		use crate::eth::tests::asymmetrise;
		use ethabi::Token;
		const FAKE_KEYMAN_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);
		const CHAIN_ID: u64 = 1;
		const NONCE: u64 = 9;

		let dummy_fetch_deploy_asset_params = vec![
			EncodableFetchDeployAssetParams {
				intent_id: 1u64,
				asset: Address::from_slice(&[3; 20]),
			},
			EncodableFetchDeployAssetParams {
				intent_id: 2u64,
				asset: Address::from_slice(&[4; 20]),
			},
		];

		let dummy_fetch_asset_params = vec![
			EncodableFetchAssetParams {
				contract_address: Address::from_slice(&[5; 20]),
				asset: Address::from_slice(&[3; 20]),
			},
			EncodableFetchAssetParams {
				contract_address: Address::from_slice(&[6; 20]),
				asset: Address::from_slice(&[4; 20]),
			},
		];

		let dummy_transfer_asset_params = vec![
			EncodableTransferAssetParams {
				asset: Address::from_slice(&[5; 20]),
				to: Address::from_slice(&[7; 20]),
				amount: 10,
			},
			EncodableTransferAssetParams {
				asset: Address::from_slice(&[6; 20]),
				to: Address::from_slice(&[8; 20]),
				amount: 20,
			},
		];

		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = asymmetrise([0x7f; 20]);
		const FAKE_SIG: [u8; 32] = asymmetrise([0xe1; 32]);

		let eth_vault = ethabi::Contract::load(
			std::include_bytes!("../../../../../engine/src/eth/abis/Vault.json").as_ref(),
		)
		.unwrap();

		let all_batch_reference = eth_vault.function("allBatch").unwrap();

		let all_batch_runtime = AllBatch::new_unsigned(
			EthereumReplayProtection {
				key_manager_address: FAKE_KEYMAN_ADDR,
				chain_id: CHAIN_ID,
				nonce: NONCE,
			},
			dummy_fetch_deploy_asset_params.clone(),
			dummy_fetch_asset_params.clone(),
			dummy_transfer_asset_params.clone(),
		);

		let expected_msg_hash = all_batch_runtime.sig_data.msg_hash;

		assert_eq!(all_batch_runtime.threshold_signature_payload(), expected_msg_hash);
		let runtime_payload = all_batch_runtime
			.clone()
			.signed(&SchnorrVerificationComponents {
				s: FAKE_SIG,
				k_times_g_address: FAKE_NONCE_TIMES_G_ADDR,
			})
			.chain_encoded();

		// Ensure signing payload isn't modified by signature.
		assert_eq!(all_batch_runtime.threshold_signature_payload(), expected_msg_hash);

		assert_eq!(
			// Our encoding:
			runtime_payload,
			// "Canonical" encoding based on the abi definition above and using the ethabi crate:
			all_batch_reference
				.encode_input(&[
					// sigData: SigData(address, uint, uint, uint, uint, address)
					Token::Tuple(vec![
						Token::Address(FAKE_KEYMAN_ADDR.into()),
						Token::Uint(CHAIN_ID.into()),
						Token::Uint(expected_msg_hash.0.into()),
						Token::Uint(FAKE_SIG.into()),
						Token::Uint(NONCE.into()),
						Token::Address(FAKE_NONCE_TIMES_G_ADDR.into()),
					]),
					dummy_fetch_deploy_asset_params.tokenize(),
					dummy_fetch_asset_params.tokenize(),
					dummy_transfer_asset_params.tokenize(),
				])
				.unwrap()
		);
	}
}
