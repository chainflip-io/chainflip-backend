use super::*;
use codec::{Decode, Encode};
use ethabi::Token;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::{vec, vec::Vec};

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
			("deployFetchParamsArray", <Vec<EncodableFetchDeployAssetParams>>::param_type()),
			("fetchParamsArray", <Vec<EncodableFetchAssetParams>>::param_type()),
			("transferParamsArray", <Vec<EncodableTransferAssetParams>>::param_type()),
		]
	}
}

#[cfg(test)]
mod test_all_batch {
	use super::*;
	use crate::{
		eth::{
			api::{abi::load_abi, EthereumReplayProtection, EthereumTransactionBuilder},
			SchnorrVerificationComponents,
		},
		ApiCall,
	};

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

		let eth_vault = load_abi("IVault");

		let all_batch_reference = eth_vault.function("allBatch").unwrap();

		let all_batch_runtime = EthereumTransactionBuilder::new_unsigned(
			EthereumReplayProtection {
				nonce: NONCE,
				chain_id: CHAIN_ID,
				key_manager_address: FAKE_KEYMAN_ADDR.into(),
				contract_address: FAKE_VAULT_ADDR.into(),
			},
			super::AllBatch::new(
				dummy_fetch_deploy_asset_params.clone(),
				dummy_fetch_asset_params.clone(),
				dummy_transfer_asset_params.clone(),
			),
		);

		let expected_msg_hash = all_batch_runtime.threshold_signature_payload();
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
