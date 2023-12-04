use super::*;
use codec::{Decode, Encode};
use ethabi::Token;
use frame_support::sp_runtime::RuntimeDebug;
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

/// Struct containing info for the TransferFallback call in the Vault contract.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct TransferFallback {
	/// The failed transfer that needs to be addressed.
	transfer_param: EncodableTransferAssetParams,
}

impl TransferFallback {
	pub(crate) fn new(transfer_param: EncodableTransferAssetParams) -> Self {
		Self { transfer_param }
	}
}

impl EvmCall for TransferFallback {
	const FUNCTION_NAME: &'static str = "transferFallback";

	fn function_params() -> Vec<(&'static str, ethabi::ParamType)> {
		vec![("transferParams", EncodableTransferAssetParams::param_type())]
	}

	fn function_call_args(&self) -> Vec<Token> {
		vec![self.transfer_param.clone().tokenize()]
	}
}

#[cfg(test)]
mod test_transfer_fallback {
	use super::*;
	use crate::{
		eth::api::abi::load_abi,
		evm::{
			api::{ApiCall, EvmReplayProtection, EvmTransactionBuilder},
			SchnorrVerificationComponents,
		},
	};
	use ethabi::Address;

	#[test]
	fn test_payload() {
		use crate::evm::tests::asymmetrise;
		use ethabi::Token;
		const FAKE_KEYMAN_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);
		const FAKE_VAULT_ADDR: [u8; 20] = asymmetrise([0xdf; 20]);
		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = asymmetrise([0x7f; 20]);
		const FAKE_SIG: [u8; 32] = asymmetrise([0xe1; 32]);
		const CHAIN_ID: u64 = 1337;
		const NONCE: u64 = 54321;

		let dummy_transfer_asset_param = EncodableTransferAssetParams {
			asset: Address::from_slice(&[5; 20]),
			to: Address::from_slice(&[7; 20]),
			amount: 10,
		};

		let eth_vault = load_abi("IVault");

		let function_reference = eth_vault.function("transferFallback").unwrap();

		let function_runtime = EvmTransactionBuilder::new_unsigned(
			EvmReplayProtection {
				nonce: NONCE,
				chain_id: CHAIN_ID,
				key_manager_address: FAKE_KEYMAN_ADDR.into(),
				contract_address: FAKE_VAULT_ADDR.into(),
			},
			super::TransferFallback::new(dummy_transfer_asset_param.clone()),
		);

		let expected_msg_hash = function_runtime.threshold_signature_payload();
		let runtime_payload = function_runtime
			.clone()
			.signed(&SchnorrVerificationComponents {
				s: FAKE_SIG,
				k_times_g_address: FAKE_NONCE_TIMES_G_ADDR,
			})
			.chain_encoded();

		// Ensure signing payload isn't modified by signature.
		assert_eq!(function_runtime.threshold_signature_payload(), expected_msg_hash);

		assert_eq!(
			// Our encoding:
			runtime_payload,
			// "Canonical" encoding based on the abi definition above and using the ethabi crate:
			function_reference
				.encode_input(&[
					// sigData: SigData(uint, uint, address)
					Token::Tuple(vec![
						Token::Uint(FAKE_SIG.into()),
						Token::Uint(NONCE.into()),
						Token::Address(FAKE_NONCE_TIMES_G_ADDR.into()),
					]),
					dummy_transfer_asset_param.tokenize(),
				])
				.unwrap()
		);
	}
}
