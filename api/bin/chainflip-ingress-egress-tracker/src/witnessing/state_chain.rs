use crate::{
	store::{Storable, Store},
	utils::{get_broadcast_id, hex_encode_bytes},
};
use cf_chains::{
	address::ToHumanreadableAddress, evm::SchnorrVerificationComponents, AnyChain, Bitcoin, Chain,
	Ethereum, Polkadot,
};
use cf_primitives::{Asset, BroadcastId, ForeignChain, NetworkEnvironment};
use chainflip_engine::state_chain_observer::client::{
	chain_api::ChainApi, storage_api::StorageApi,
};
use pallet_cf_ingress_egress::DepositWitness;
use serde::Serialize;
use utilities::rpc::NumberOrHex;

#[derive(Serialize)]
#[serde(untagged)]
enum TransactionId {
	Bitcoin { hash: String },
	Ethereum { signature: SchnorrVerificationComponents },
	Polkadot { signature: String },
}

#[derive(Serialize)]
#[serde(untagged)]
enum WitnessInformation {
	Deposit {
		deposit_chain_block_height: <AnyChain as Chain>::ChainBlockNumber,
		#[serde(skip_serializing)]
		deposit_address: String,
		amount: NumberOrHex,
		asset: Asset,
	},
	Broadcast {
		#[serde(skip_serializing)]
		broadcast_id: BroadcastId,
		tx_out_id: TransactionId,
	},
}

impl WitnessInformation {
	fn to_foreign_chain(&self) -> ForeignChain {
		match self {
			Self::Deposit { asset, .. } => (*asset).into(),
			Self::Broadcast { tx_out_id, .. } => match tx_out_id {
				TransactionId::Bitcoin { .. } => ForeignChain::Bitcoin,
				TransactionId::Ethereum { .. } => ForeignChain::Ethereum,
				TransactionId::Polkadot { .. } => ForeignChain::Polkadot,
			},
		}
	}
}

impl Storable for WitnessInformation {
	fn get_key(&self) -> String {
		let chain = self.to_foreign_chain().to_string();

		match self {
			Self::Deposit { deposit_address, .. } => {
				format!("deposit:{chain}:{deposit_address}")
			},
			Self::Broadcast { broadcast_id, .. } => {
				format!("broadcast:{chain}:{broadcast_id}")
			},
		}
	}

	fn get_expiry_duration(&self) -> std::time::Duration {
		match self.to_foreign_chain() {
			ForeignChain::Bitcoin => std::time::Duration::from_secs(3600 * 6),
			_ => Self::DEFAULT_EXPIRY_DURATION,
		}
	}
}

type DepositInfo<T> = (DepositWitness<T>, <T as Chain>::ChainBlockNumber, NetworkEnvironment);

impl From<DepositInfo<Ethereum>> for WitnessInformation {
	fn from((value, height, _): DepositInfo<Ethereum>) -> Self {
		Self::Deposit {
			deposit_chain_block_height: height,
			deposit_address: hex_encode_bytes(value.deposit_address.as_bytes()),
			amount: value.amount.into(),
			asset: value.asset.into(),
		}
	}
}

impl From<DepositInfo<Bitcoin>> for WitnessInformation {
	fn from((value, height, network): DepositInfo<Bitcoin>) -> Self {
		Self::Deposit {
			deposit_chain_block_height: height,
			deposit_address: value.deposit_address.to_humanreadable(network),
			amount: value.amount.into(),
			asset: value.asset.into(),
		}
	}
}

impl From<DepositInfo<Polkadot>> for WitnessInformation {
	fn from((value, height, _): DepositInfo<Polkadot>) -> Self {
		Self::Deposit {
			deposit_chain_block_height: height as u64,
			deposit_address: hex_encode_bytes(value.deposit_address.aliased_ref()),
			amount: value.amount.into(),
			asset: value.asset.into(),
		}
	}
}

pub async fn handle_call<S, StateChainClient>(
	call: state_chain_runtime::RuntimeCall,
	store: &mut S,
	chainflip_network: NetworkEnvironment,
	state_chain_client: &StateChainClient,
) -> anyhow::Result<()>
where
	S: Store,
	StateChainClient: StorageApi + ChainApi + 'static + Send + Sync,
{
	use pallet_cf_broadcast::Call as BroadcastCall;
	use pallet_cf_ingress_egress::Call as IngressEgressCall;
	use state_chain_runtime::RuntimeCall::*;

	match call {
		EthereumIngressEgress(IngressEgressCall::process_deposits {
			deposit_witnesses,
			block_height,
		}) =>
			for witness in deposit_witnesses as Vec<DepositWitness<Ethereum>> {
				store
					.save_to_array(&WitnessInformation::from((
						witness,
						block_height,
						chainflip_network,
					)))
					.await?;
			},
		BitcoinIngressEgress(IngressEgressCall::process_deposits {
			deposit_witnesses,
			block_height,
		}) =>
			for witness in deposit_witnesses as Vec<DepositWitness<Bitcoin>> {
				store
					.save_to_array(&WitnessInformation::from((
						witness,
						block_height,
						chainflip_network,
					)))
					.await?;
			},
		PolkadotIngressEgress(IngressEgressCall::process_deposits {
			deposit_witnesses,
			block_height,
		}) =>
			for witness in deposit_witnesses as Vec<DepositWitness<Polkadot>> {
				store
					.save_to_array(&WitnessInformation::from((
						witness,
						block_height,
						chainflip_network,
					)))
					.await?;
			},
		EthereumBroadcaster(BroadcastCall::transaction_succeeded { tx_out_id, .. }) => {
			let broadcast_id =
				get_broadcast_id::<Ethereum, StateChainClient>(state_chain_client, &tx_out_id)
					.await;

			if let Some(broadcast_id) = broadcast_id {
				store
					.save_singleton(&WitnessInformation::Broadcast {
						broadcast_id,
						tx_out_id: TransactionId::Ethereum { signature: tx_out_id },
					})
					.await?;
			}
		},
		BitcoinBroadcaster(BroadcastCall::transaction_succeeded { tx_out_id, .. }) => {
			let broadcast_id =
				get_broadcast_id::<Bitcoin, StateChainClient>(state_chain_client, &tx_out_id).await;

			if let Some(broadcast_id) = broadcast_id {
				store
					.save_singleton(&WitnessInformation::Broadcast {
						broadcast_id,
						tx_out_id: TransactionId::Bitcoin {
							hash: format!("0x{}", hex::encode(tx_out_id)),
						},
					})
					.await?;
			}
		},
		PolkadotBroadcaster(BroadcastCall::transaction_succeeded { tx_out_id, .. }) => {
			let broadcast_id =
				get_broadcast_id::<Polkadot, StateChainClient>(state_chain_client, &tx_out_id)
					.await;

			if let Some(broadcast_id) = broadcast_id {
				store
					.save_singleton(&WitnessInformation::Broadcast {
						broadcast_id,
						tx_out_id: TransactionId::Polkadot {
							signature: format!("0x{}", hex::encode(tx_out_id.aliased_ref())),
						},
					})
					.await?;
			}
		},

		EthereumIngressEgress(_) |
		BitcoinIngressEgress(_) |
		PolkadotIngressEgress(_) |
		System(_) |
		Timestamp(_) |
		Environment(_) |
		Flip(_) |
		Emissions(_) |
		Funding(_) |
		AccountRoles(_) |
		Witnesser(_) |
		Validator(_) |
		Session(_) |
		Grandpa(_) |
		Governance(_) |
		Reputation(_) |
		TokenholderGovernance(_) |
		EthereumChainTracking(_) |
		BitcoinChainTracking(_) |
		PolkadotChainTracking(_) |
		EthereumVault(_) |
		PolkadotVault(_) |
		BitcoinVault(_) |
		EthereumThresholdSigner(_) |
		PolkadotThresholdSigner(_) |
		BitcoinThresholdSigner(_) |
		EthereumBroadcaster(_) |
		PolkadotBroadcaster(_) |
		BitcoinBroadcaster(_) |
		Swapping(_) |
		LiquidityProvider(_) |
		LiquidityPools(_) => {},
	};

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::anyhow;
	use async_trait::async_trait;
	use cf_chains::{
		dot::PolkadotAccountId,
		evm::{EvmTransactionMetadata, TransactionFee},
		Chain,
	};
	use cf_primitives::{BroadcastId, NetworkEnvironment};
	use chainflip_engine::state_chain_observer::client::{
		chain_api::ChainApi,
		storage_api,
		storage_api::StorageApi,
		stream_api::{StreamApi, FINALIZED, UNFINALIZED},
		BlockInfo,
	};
	use frame_support::storage::types::QueryKindTrait;
	use jsonrpsee::core::RpcResult;
	use mockall::mock;
	use pallet_cf_ingress_egress::DepositWitness;
	use sp_core::{storage::StorageKey, H160};
	use state_chain_runtime::PalletInstanceAlias;
	use std::collections::HashMap;

	#[derive(Default)]
	struct MockStore {
		storage: HashMap<String, serde_json::Value>,
	}

	#[async_trait]
	impl Store for MockStore {
		type Output = ();

		async fn save_to_array<S: Storable>(
			&mut self,
			storable: &S,
		) -> anyhow::Result<Self::Output> {
			let key = storable.get_key();
			let value = serde_json::to_value(storable)?;

			let array = self.storage.entry(key).or_insert(serde_json::Value::Array(vec![]));

			array.as_array_mut().ok_or(anyhow!("expect array"))?.push(value);

			Ok(())
		}

		async fn save_singleton<S: Storable>(
			&mut self,
			storable: &S,
		) -> anyhow::Result<Self::Output> {
			let key = storable.get_key();

			let value = serde_json::to_value(storable)?;

			self.storage.insert(key, value);

			Ok(())
		}
	}

	mock! {
		pub StateChainClient {}
		#[async_trait]
		impl ChainApi for StateChainClient {
			fn latest_finalized_block(&self) -> BlockInfo;
			fn latest_unfinalized_block(&self) -> BlockInfo;

			async fn finalized_block_stream(&self) -> Box<dyn StreamApi<FINALIZED>>;
			async fn unfinalized_block_stream(&self) -> Box<dyn StreamApi<UNFINALIZED>>;

			async fn block(&self, block_hash: state_chain_runtime::Hash) -> RpcResult<BlockInfo>;
		}

		#[async_trait]
		impl StorageApi for StateChainClient {
			async fn storage_item<
				Value: codec::FullCodec + 'static,
				OnEmpty: 'static,
				QueryKind: QueryKindTrait<Value, OnEmpty> + 'static,
			>(
				&self,
				storage_key: StorageKey,
				block_hash: state_chain_runtime::Hash,
			) -> RpcResult<<QueryKind as QueryKindTrait<Value, OnEmpty>>::Query>;

			async fn storage_value<StorageValue: storage_api::StorageValueAssociatedTypes + 'static>(
				&self,
				block_hash: state_chain_runtime::Hash,
			) -> RpcResult<<StorageValue::QueryKind as QueryKindTrait<StorageValue::Value, StorageValue::OnEmpty>>::Query>;

			async fn storage_map_entry<StorageMap: storage_api::StorageMapAssociatedTypes + 'static>(
				&self,
				block_hash: state_chain_runtime::Hash,
				key: &StorageMap::Key,
			) -> RpcResult<
				<StorageMap::QueryKind as QueryKindTrait<StorageMap::Value, StorageMap::OnEmpty>>::Query,
			>
			where
				StorageMap::Key: Sync;

			async fn storage_double_map_entry<StorageDoubleMap: storage_api::StorageDoubleMapAssociatedTypes + 'static>(
				&self,
				block_hash: state_chain_runtime::Hash,
				key1: &StorageDoubleMap::Key1,
				key2: &StorageDoubleMap::Key2,
			) -> RpcResult<
				<StorageDoubleMap::QueryKind as QueryKindTrait<
					StorageDoubleMap::Value,
					StorageDoubleMap::OnEmpty,
				>>::Query,
			>
			where
				StorageDoubleMap::Key1: Sync,
				StorageDoubleMap::Key2: Sync;

			async fn storage_map<StorageMap: storage_api::StorageMapAssociatedTypes + 'static, ReturnedIter: FromIterator<(<StorageMap as storage_api::StorageMapAssociatedTypes>::Key, StorageMap::Value)> + 'static>(
				&self,
				block_hash: state_chain_runtime::Hash,
			) -> RpcResult<ReturnedIter>;
		}
	}

	#[allow(clippy::type_complexity)]
	fn create_client<I>(
		result: Option<(
			BroadcastId,
			<<state_chain_runtime::Runtime as pallet_cf_broadcast::Config<I::Instance>>::TargetChain as Chain>::ChainBlockNumber,
		)>,
	) -> MockStateChainClient
	where
		state_chain_runtime::Runtime: pallet_cf_broadcast::Config<I::Instance>,
		I: PalletInstanceAlias + 'static,
	{
		let mut client = MockStateChainClient::new();

		client
			.expect_storage_map_entry::<pallet_cf_broadcast::TransactionOutIdToBroadcastId<
				state_chain_runtime::Runtime,
				I::Instance,
			>>()
			.return_once(move |_, _| Ok(result));

		client.expect_latest_unfinalized_block().returning(|| BlockInfo {
			parent_hash: state_chain_runtime::Hash::default(),
			hash: state_chain_runtime::Hash::default(),
			number: 1,
		});

		client
	}

	fn parse_eth_address(address: &'static str) -> (H160, &'static str) {
		let mut eth_address_bytes = [0; 20];

		for (index, byte) in hex::decode(&address[2..]).unwrap().into_iter().enumerate() {
			eth_address_bytes[index] = byte;
		}

		(H160::from(eth_address_bytes), address)
	}

	#[tokio::test]
	async fn test_handle_deposit_calls() {
		let polkadot_address = "14JWPRWMkEyLLdrN2k3teBd446sKKJuwU2DDKw4Ev4dYcHeF";
		let polkadot_account_id = polkadot_address.parse::<PolkadotAccountId>().unwrap();

		let (eth_address1, eth_address_str1) =
			parse_eth_address("0x541f563237A309B3A61E33BDf07a8930Bdba8D99");

		let (eth_address2, eth_address_str2) =
			parse_eth_address("0xa56A6be23b6Cf39D9448FF6e897C29c41c8fbDFF");

		let client = MockStateChainClient::new();
		let mut store = MockStore::default();
		handle_call(
			state_chain_runtime::RuntimeCall::EthereumIngressEgress(
				pallet_cf_ingress_egress::Call::process_deposits {
					deposit_witnesses: vec![DepositWitness {
						deposit_address: eth_address1,
						amount: 100u128,
						asset: cf_chains::assets::eth::Asset::Eth,
						deposit_details: (),
					}],
					block_height: 1,
				},
			),
			&mut store,
			NetworkEnvironment::Testnet,
			&client,
		)
		.await
		.expect("failed to handle call");
		handle_call(
			state_chain_runtime::RuntimeCall::PolkadotIngressEgress(
				pallet_cf_ingress_egress::Call::process_deposits {
					deposit_witnesses: vec![DepositWitness {
						deposit_address: polkadot_account_id,
						amount: 100u128,
						asset: cf_chains::assets::dot::Asset::Dot,
						deposit_details: (),
					}],
					block_height: 1,
				},
			),
			&mut store,
			NetworkEnvironment::Testnet,
			&client,
		)
		.await
		.expect("failed to handle call");
		handle_call(
			state_chain_runtime::RuntimeCall::EthereumIngressEgress(
				pallet_cf_ingress_egress::Call::process_deposits {
					deposit_witnesses: vec![DepositWitness {
						deposit_address: eth_address2,
						amount: 100u128,
						asset: cf_chains::assets::eth::Asset::Eth,
						deposit_details: (),
					}],
					block_height: 1,
				},
			),
			&mut store,
			NetworkEnvironment::Testnet,
			&client,
		)
		.await
		.expect("failed to handle call");

		assert_eq!(store.storage.len(), 3);
		println!("{:?}", store.storage);
		insta::assert_display_snapshot!(store
			.storage
			.get(format!("deposit:Ethereum:{}", eth_address_str1.to_lowercase()).as_str())
			.unwrap());
		insta::assert_display_snapshot!(store
			.storage
			.get(
				format!(
					"deposit:Polkadot:{}",
					format!("0x{}", hex::encode(polkadot_account_id.aliased_ref()))
				)
				.as_str()
			)
			.unwrap());
		insta::assert_display_snapshot!(store
			.storage
			.get(format!("deposit:Ethereum:{}", eth_address_str2.to_lowercase()).as_str())
			.unwrap());

		handle_call(
			state_chain_runtime::RuntimeCall::EthereumIngressEgress(
				pallet_cf_ingress_egress::Call::process_deposits {
					deposit_witnesses: vec![DepositWitness {
						deposit_address: eth_address1,
						amount: 2_000_000u128,
						asset: cf_chains::assets::eth::Asset::Eth,
						deposit_details: (),
					}],
					block_height: 1,
				},
			),
			&mut store,
			NetworkEnvironment::Testnet,
			&client,
		)
		.await
		.expect("failed to handle call");
		assert_eq!(store.storage.len(), 3);
		insta::assert_display_snapshot!(store
			.storage
			.get(format!("deposit:Ethereum:{}", eth_address_str1.to_lowercase()).as_str())
			.unwrap());
	}

	#[tokio::test]
	async fn test_handle_broadcast_calls() {
		let (eth_address, _) = parse_eth_address("0x541f563237A309B3A61E33BDf07a8930Bdba8D99");

		let tx_out_id = SchnorrVerificationComponents { s: [0; 32], k_times_g_address: [0; 20] };

		let client = create_client::<Ethereum>(Some((1, 2)));
		let mut store = MockStore::default();
		handle_call(
			state_chain_runtime::RuntimeCall::EthereumBroadcaster(
				pallet_cf_broadcast::Call::transaction_succeeded {
					tx_out_id,
					signer_id: eth_address,
					tx_fee: TransactionFee { gas_used: 0, effective_gas_price: 0 },
					tx_metadata: EvmTransactionMetadata {
						max_fee_per_gas: None,
						max_priority_fee_per_gas: None,
						contract: H160::from([0; 20]),
						gas_limit: None,
					},
				},
			),
			&mut store,
			NetworkEnvironment::Testnet,
			&client,
		)
		.await
		.expect("failed to handle call");

		assert_eq!(store.storage.len(), 1);
		insta::assert_display_snapshot!(store.storage.get("broadcast:Ethereum:1").unwrap());
	}
}
