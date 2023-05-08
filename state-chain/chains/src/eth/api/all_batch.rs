use crate::eth::{deposit_address::get_salt, EthereumCall, SigData, Tokenizable};
use cf_primitives::{AssetAmount, ChannelId};
use codec::{Decode, Encode};
use ethabi::{encode, Address, ParamType, Token, Uint};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{Hash, Keccak256},
	RuntimeDebug,
};
use sp_std::{boxed::Box, vec, vec::Vec};

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub(crate) struct EncodableFetchAssetParams {
	pub contract_address: Address,
	pub asset: Address,
}

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub(crate) struct EncodableFetchDeployAssetParams {
	pub channel_id: ChannelId,
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
			Token::FixedBytes(get_salt(self.channel_id).to_vec()),
			Token::Address(self.asset),
		])
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Array(Box::new(ParamType::Tuple(vec![
			ParamType::FixedBytes(32),
			ParamType::Address,
		])))
	}
}
impl Tokenizable for EncodableFetchAssetParams {
	fn tokenize(self) -> Token {
		Token::Tuple(vec![Token::Address(self.contract_address), Token::Address(self.asset)])
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Array(Box::new(ParamType::Tuple(vec![ParamType::Address, ParamType::Address])))
	}
}

impl<T: Tokenizable> Tokenizable for Vec<T> {
	fn tokenize(self) -> Token {
		Token::Array(self.into_iter().map(|t| t.tokenize()).collect())
	}

	fn param_type() -> ethabi::ParamType {
		todo!()
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

	fn param_type() -> ethabi::ParamType {
		ParamType::Array(Box::new(ParamType::Tuple(vec![
			ParamType::Address,
			ParamType::Address,
			ParamType::Uint(256),
		])))
	}
}

/// Represents all the arguments required to build the call to Vault's 'allBatch'
/// function.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct AllBatch {
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
	pub(crate) fn new(
		fetch_deploy_params: Vec<EncodableFetchDeployAssetParams>,
		fetch_params: Vec<EncodableFetchAssetParams>,
		transfer_params: Vec<EncodableTransferAssetParams>,
	) -> Self {
		Self { fetch_deploy_params, fetch_params, transfer_params }
	}
}

impl EthereumCall for AllBatch {
	const FUNCTION_NAME: &'static str = "allBatch";

	fn function_call_args(&self) -> Vec<Token> {
		vec![
			self.fetch_deploy_params.clone().tokenize(),
			self.fetch_params.clone().tokenize(),
			self.transfer_params.clone().tokenize(),
		]
	}

	fn function_params() -> Vec<(&'static str, ethabi::ParamType)> {
		vec![
			("deployFetchParamsArray", EncodableFetchDeployAssetParams::param_type()),
			("fetchParamsArray", EncodableFetchAssetParams::param_type()),
			("transferParamsArray", EncodableTransferAssetParams::param_type()),
		]
	}
}

#[cfg(test)]
mod test_all_batch {
	use super::*;
	use crate::{
		eth::{
			api::EthereumReplayProtection, EthereumTransactionBuilder,
			SchnorrVerificationComponents,
		},
		ApiCall,
	};
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
		const FAKE_VAULT_ADDR: [u8; 20] = asymmetrise([0xdf; 20]);
		const CHAIN_ID: u64 = 1;
		const NONCE: u64 = 9;

		let dummy_fetch_deploy_asset_params = vec![
			EncodableFetchDeployAssetParams {
				channel_id: 1u64,
				asset: Address::from_slice(&[3; 20]),
			},
			EncodableFetchDeployAssetParams {
				channel_id: 2u64,
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

		let call = AllBatch::new(
			dummy_fetch_deploy_asset_params.clone(),
			dummy_fetch_asset_params.clone(),
			dummy_transfer_asset_params.clone(),
		);
		let expected_msg_hash = call.msg_hash();
		let all_batch_runtime = EthereumTransactionBuilder::new_unsigned(
			EthereumReplayProtection {
				nonce: NONCE,
				chain_id: CHAIN_ID,
				key_manager_address: FAKE_KEYMAN_ADDR.into(),
				contract_address: FAKE_VAULT_ADDR.into(),
			},
			call,
		);

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
					// sigData: SigData(uint, uint, address)
					Token::Tuple(vec![
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
