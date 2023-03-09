use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

use crate::btc::{Bitcoin, BitcoinOutput, BitcoinTransaction, Utxo};

use crate::{ApiCall, ChainCrypto};

use sp_runtime::RuntimeDebug;

/// Represents all the arguments required to build the call to fetch assets for all given intent
/// ids.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct BatchFetchAndTransfer {
	/// The handler for creating and signing polkadot extrinsics
	pub bitcoin_transaction: BitcoinTransaction,
	/// The list of all inbound deposits that are to be fetched in this batch call.
	pub input_utxos: Vec<Utxo>,
	/// The list of all outbound transfers that are to be executed in this call.
	pub outputs: Vec<BitcoinOutput>,
}

impl BatchFetchAndTransfer {
	pub fn new_unsigned(input_utxos: Vec<Utxo>, outputs: Vec<BitcoinOutput>) -> Self {
		Self {
			bitcoin_transaction: BitcoinTransaction::create_new_unsigned(
				input_utxos.clone(),
				outputs.clone(),
			),
			input_utxos,
			outputs,
		}
	}
}

impl ApiCall<Bitcoin> for BatchFetchAndTransfer {
	fn threshold_signature_payload(&self) -> <Bitcoin as ChainCrypto>::Payload {
		let mut payloads = vec![];
		for i in 0..self.input_utxos.len() {
			payloads.push(self.bitcoin_transaction.get_signing_payload(i as u32))
		}
		payloads
	}

	fn signed(mut self, signatures: &<Bitcoin as ChainCrypto>::ThresholdSignature) -> Self {
		for (i, signature) in signatures.iter().enumerate() {
			self.bitcoin_transaction.add_signature(i as u32, *signature);
		}
		self
	}

	fn chain_encoded(&self) -> Vec<u8> {
		self.bitcoin_transaction.clone().finalize()
	}

	fn is_signed(&self) -> bool {
		self.bitcoin_transaction.is_signed()
	}
}

// #[cfg(test)]
// mod test_batch_fetch {

// 	use super::*;
// 	use crate::dot::{sr25519::Pair, NONCE_1, RAW_SEED_1, RAW_SEED_2, TEST_RUNTIME_VERSION};
// 	use cf_primitives::chains::assets;
// 	use sp_core::{
// 		crypto::{AccountId32, Pair as TraitPair},
// 		sr25519, Hasher,
// 	};
// 	use sp_runtime::{
// 		traits::{BlakeTwo256, IdentifyAccount},
// 		MultiSigner,
// 	};

// 	#[ignore]
// 	#[test]
// 	fn create_test_api_call() {
// 		let keypair_vault: Pair = <Pair as TraitPair>::from_seed(&RAW_SEED_1);
// 		let account_id_vault: AccountId32 =
// 			MultiSigner::Sr25519(keypair_vault.public()).into_account();

// 		let keypair_proxy: Pair = <Pair as TraitPair>::from_seed(&RAW_SEED_2);
// 		let account_id_proxy: AccountId32 =
// 			MultiSigner::Sr25519(keypair_proxy.public()).into_account();

// 		let dummy_fetch_params: Vec<FetchAssetParams<Polkadot>> = vec![
// 			FetchAssetParams::<Polkadot> { ingress_fetch_id: 1, asset: assets::dot::Asset::Dot },
// 			FetchAssetParams::<Polkadot> { ingress_fetch_id: 2, asset: assets::dot::Asset::Dot },
// 			FetchAssetParams::<Polkadot> { ingress_fetch_id: 3, asset: assets::dot::Asset::Dot },
// 		];

// 		let dummy_transfer_params: Vec<TransferAssetParams<Polkadot>> = vec![
// 			TransferAssetParams::<Polkadot> {
// 				to: MultiSigner::Sr25519(sr25519::Public([7u8; 32])).into_account(),
// 				amount: 4,
// 				asset: assets::dot::Asset::Dot,
// 			},
// 			TransferAssetParams::<Polkadot> {
// 				to: MultiSigner::Sr25519(sr25519::Public([8u8; 32])).into_account(),
// 				amount: 5,
// 				asset: assets::dot::Asset::Dot,
// 			},
// 			TransferAssetParams::<Polkadot> {
// 				to: MultiSigner::Sr25519(sr25519::Public([9u8; 32])).into_account(),
// 				amount: 6,
// 				asset: assets::dot::Asset::Dot,
// 			},
// 		];

// 		let batch_fetch_api = BatchFetchAndTransfer::new_unsigned(
// 			PolkadotReplayProtection::new(NONCE_1, 0, TEST_RUNTIME_VERSION, Default::default()),
// 			dummy_fetch_params,
// 			dummy_transfer_params,
// 			account_id_proxy,
// 			account_id_vault,
// 		);

// 		println!(
// 			"CallHash: 0x{}",
// 			batch_fetch_api
// 				.extrinsic_builder
// 				.extrinsic_call
// 				.using_encoded(|encoded| hex::encode(BlakeTwo256::hash(encoded)))
// 		);
// 		println!(
// 			"Encoded Call: 0x{}",
// 			hex::encode(batch_fetch_api.extrinsic_builder.extrinsic_call.encode())
// 		);

// 		let batch_fetch_api = batch_fetch_api
// 			.clone()
// 			.signed(&keypair_proxy.sign(&batch_fetch_api.threshold_signature_payload().0));
// 		assert!(batch_fetch_api.is_signed());

// 		println!("encoded extrinsic: 0x{}", hex::encode(batch_fetch_api.chain_encoded()));
// 	}
// }
