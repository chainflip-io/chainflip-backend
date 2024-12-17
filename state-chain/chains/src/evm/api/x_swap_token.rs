use super::*;
use crate::cf_parameters::VersionedCfParameters;
use address::to_encoded_address;
use cf_primitives::{Asset, ForeignChain, NetworkEnvironment};
use codec::{Decode, Encode};
use ethabi::Token;
use frame_support::sp_runtime::RuntimeDebug;
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

/// Represents all the arguments required to build the call to Vault's 'ExecutexSwapAndCall'
/// function.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct XSwapToken {
	/// The destination chain according to Chainflip Protocol's nomenclature.
	dst_chain: u32,
	/// Bytes containing the destination address on the destination chain.
	dst_address: Vec<u8>,
	/// Destination token to be swapped to.
	dst_token: u32,
	/// Address of the source token to swap.
	src_token: EvmAddress,
	/// Amount of source tokens to swap.
	amount: U256,
	/// Additional parameters to be passed to the Chainflip Protocol.
	cf_parameters: Vec<u8>,
}

pub enum Error {
	UnsupportedSourceAsset,
}

impl XSwapToken {
	pub fn new<SrcChain, EvmEnvironment, DstChain, GetNetwork, CcmData>(
		destination_address: DstChain::ChainAccount,
		destination_asset: DstChain::ChainAsset,
		source_asset: SrcChain::ChainAsset,
		source_amount: AssetAmount,
		cf_parameters: VersionedCfParameters<CcmData>,
	) -> Result<Self, Error>
	where
		DstChain: Chain + Get<ForeignChain>,
		GetNetwork: Get<NetworkEnvironment>,
		SrcChain: Chain<ChainCrypto = EvmCrypto>,
		EvmEnvironment: EvmEnvironmentProvider<SrcChain>,
		CcmData: Encode,
	{
		Ok(Self {
			dst_chain: DstChain::get() as u32,
			dst_address: to_encoded_address(
				<DstChain::ChainAccount as IntoForeignChainAddress<DstChain>>::into_foreign_chain_address(
					destination_address,
				),
				GetNetwork::get,
			)
			.inner_bytes()
			.to_vec(),
			dst_token: Into::<Asset>::into(destination_asset) as u32,
			src_token: EvmEnvironment::token_address(source_asset)
				.ok_or(Error::UnsupportedSourceAsset)?,
			amount: source_amount.into(),
			cf_parameters: cf_parameters.encode(),
		})
	}
}

impl EvmCall for XSwapToken {
	const FUNCTION_NAME: &'static str = "xSwapToken";

	fn function_params() -> Vec<(&'static str, ethabi::ParamType)> {
		vec![
			("dstChain", u32::param_type()),
			("dstAddress", <Vec<u8>>::param_type()),
			("dstToken", u32::param_type()),
			("srcToken", ethabi::Address::param_type()),
			("amount", U256::param_type()),
			("cfParameters", <Vec<u8>>::param_type()),
		]
	}

	fn function_call_args(&self) -> Vec<Token> {
		vec![
			self.dst_chain.tokenize(),
			self.dst_address.clone().tokenize(),
			self.dst_token.tokenize(),
			self.src_token.tokenize(),
			self.amount.tokenize(),
			self.cf_parameters.clone().tokenize(),
		]
	}
}

// #[cfg(test)]
// mod test {
// 	use super::*;
// 	use crate::{
// 		dot::PolkadotAccountId,
// 		eth::api::abi::load_abi,
// 		evm::{
// 			api::{EvmReplayProtection, EvmTransactionBuilder},
// 			SchnorrVerificationComponents,
// 		},
// 		ForeignChainAddress,
// 	};
// 	use ethabi::Address;

// 	#[test]
// 	fn test_payload() {
// 		use crate::evm::tests::asymmetrise;
// 		use ethabi::Token;
// 		const FAKE_KEYMAN_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);
// 		const FAKE_VAULT_ADDR: [u8; 20] = asymmetrise([0xdf; 20]);
// 		const CHAIN_ID: u64 = 1;
// 		const NONCE: u64 = 9;
// 		const GAS_BUDGET: <Ethereum as Chain>::ChainAmount = 100_000u128;

// 		let dummy_transfer_asset_param = EncodableTransferAssetParams {
// 			asset: Address::from_slice(&[5; 20]),
// 			to: Address::from_slice(&[7; 20]),
// 			amount: 10,
// 		};

// 		let dummy_src_address =
// 			ForeignChainAddress::Dot(PolkadotAccountId::from_aliased([0xff; 32]));
// 		let dummy_src_chain = ForeignChain::Polkadot;
// 		let dummy_chain = dummy_src_chain as u32;
// 		let dummy_address = ForeignChainAddress::to_source_address(dummy_src_address.clone());
// 		let dummy_message = vec![0x00, 0x01, 0x02, 0x03, 0x04];

// 		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = asymmetrise([0x7f; 20]);
// 		const FAKE_SIG: [u8; 32] = asymmetrise([0xe1; 32]);

// 		let eth_vault = load_abi("IVault");

// 		let function_reference = eth_vault.function("executexSwapAndCall").unwrap();

// 		let function_runtime = EvmTransactionBuilder::new_unsigned(
// 			EvmReplayProtection {
// 				nonce: NONCE,
// 				chain_id: CHAIN_ID,
// 				key_manager_address: FAKE_KEYMAN_ADDR.into(),
// 				contract_address: FAKE_VAULT_ADDR.into(),
// 			},
// 			super::XSwapToken::new(todo!(), todo!(), todo!(), todo!(), todo!()).unwrap(),
// 		);

// 		let expected_msg_hash = function_runtime.threshold_signature_payload();
// 		let runtime_payload = function_runtime
// 			.clone()
// 			.signed(
// 				&SchnorrVerificationComponents {
// 					s: FAKE_SIG,
// 					k_times_g_address: FAKE_NONCE_TIMES_G_ADDR,
// 				},
// 				Default::default(),
// 			)
// 			.chain_encoded();

// 		// Ensure signing payload isn't modified by signature.
// 		assert_eq!(function_runtime.threshold_signature_payload(), expected_msg_hash);

// 		assert_eq!(
// 			// Our encoding:
// 			runtime_payload,
// 			// "Canonical" encoding based on the abi definition above and using the ethabi crate:
// 			function_reference
// 				.encode_input(&[
// 					// sigData: SigData(uint, uint, address)
// 					Token::Tuple(vec![
// 						Token::Uint(FAKE_SIG.into()),
// 						Token::Uint(NONCE.into()),
// 						Token::Address(FAKE_NONCE_TIMES_G_ADDR.into()),
// 					]),
// 					dummy_transfer_asset_param.tokenize(),
// 					dummy_chain.tokenize(),
// 					dummy_address.tokenize(),
// 					dummy_message.tokenize(),
// 				])
// 				.unwrap()
// 		);
// 	}
// }
