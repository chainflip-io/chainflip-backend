use crate::{
	store::{Storable, Store},
	utils::{get_broadcast_id, hex_encode_bytes},
};
use cf_chains::{
	address::{IntoForeignChainAddress, ToHumanreadableAddress},
	dot::{PolkadotExtrinsicIndex, PolkadotTransactionId},
	evm::{SchnorrVerificationComponents, H256},
	instances::ChainInstanceFor,
	AnyChain, Arbitrum, Bitcoin, CcmDepositMetadata, Chain, ChainCrypto, ChannelRefundParameters,
	Ethereum, IntoTransactionInIdForAnyChain, Polkadot, TransactionInIdForAnyChain,
};
use cf_utilities::{rpc::NumberOrHex, ArrayCollect};
use chainflip_api::primitives::{
	AffiliateShortId, Affiliates, BasisPoints, Beneficiary, BroadcastId, DcaParameters,
	ForeignChain, NetworkEnvironment,
};
use chainflip_engine::state_chain_observer::client::{
	chain_api::ChainApi, storage_api::StorageApi, StateChainClient,
};
use codec::Encode;
use pallet_cf_broadcast::TransactionOutIdFor;
use pallet_cf_ingress_egress::{DepositWitness, VaultDepositWitness};
use serde::{Serialize, Serializer};
use sp_core::{bounded::alloc::sync::Arc, crypto::AccountId32, Get};
use std::collections::BTreeMap;

/// A wrapper type for bitcoin hashes that serializes the hash in reverse.
#[derive(Debug)]
struct BitcoinHash(pub H256);

impl Serialize for BitcoinHash {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		H256(self.0.to_fixed_bytes().into_iter().rev().collect_array()).serialize(serializer)
	}
}

struct DotSignature([u8; 64]);

impl Serialize for DotSignature {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		format!("0x{}", hex::encode(self.0)).serialize(serializer)
	}
}

#[derive(Serialize)]
#[serde(untagged)]
enum TransactionRef {
	Bitcoin { hash: BitcoinHash },
	Ethereum { hash: H256 },
	Polkadot { transaction_id: PolkadotTransactionId },
	Arbitrum { hash: H256 },
}

#[derive(Serialize)]
#[serde(untagged)]
enum TransactionId {
	Bitcoin { hash: BitcoinHash },
	Ethereum { signature: SchnorrVerificationComponents },
	Polkadot { signature: DotSignature },
	Arbitrum { signature: SchnorrVerificationComponents },
}

#[derive(Serialize)]
#[serde(untagged)]
enum DepositDetails {
	Bitcoin { tx_id: H256, vout: u32 },
	Ethereum { tx_hashes: Vec<H256> },
	Polkadot { extrinsic_index: PolkadotExtrinsicIndex },
	Arbitrum { tx_hashes: Vec<H256> },
}

struct BroadcastDetails<I: cf_chains::instances::ChainInstanceAlias + Chain + 'static>
where
	state_chain_runtime::Runtime: pallet_cf_broadcast::Config<ChainInstanceFor<I>>,
{
	broadcast_id: BroadcastId,
	tx_out_id: TransactionOutIdFor<state_chain_runtime::Runtime, ChainInstanceFor<I>>,
	tx_ref: I::TransactionRef,
}

#[allow(clippy::large_enum_variant)]
#[derive(Serialize)]
#[serde(untagged)]
enum WitnessInformation {
	Deposit {
		deposit_chain_block_height: <AnyChain as Chain>::ChainBlockNumber,
		#[serde(skip_serializing)]
		deposit_address: String,
		amount: NumberOrHex,
		asset: cf_chains::assets::any::Asset,
		deposit_details: Option<DepositDetails>,
	},
	Broadcast {
		#[serde(skip_serializing)]
		broadcast_id: BroadcastId,
		tx_out_id: TransactionId,
		tx_ref: TransactionRef,
	},
	VaultDeposit {
		#[serde(skip_serializing)]
		tx_id: TransactionInIdForAnyChain,
		deposit_chain_block_height: <AnyChain as Chain>::ChainBlockNumber,
		input_asset: cf_chains::assets::any::Asset,
		output_asset: cf_chains::assets::any::Asset,
		amount: NumberOrHex,
		destination_address: String,
		ccm_deposit_metadata: Option<CcmDepositMetadata>,
		deposit_details: Option<DepositDetails>,
		broker_fee: Beneficiary<AccountId32>,
		affiliate_fees: Affiliates<AccountId32>,
		refund_params: ChannelRefundParameters,
		dca_params: Option<DcaParameters>,
		boost_fee: BasisPoints,
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
				TransactionId::Arbitrum { .. } => ForeignChain::Arbitrum,
			},
			Self::VaultDeposit { input_asset: asset, .. } => (*asset).into(),
		}
	}

	async fn save_to_store<S: Store>(&self, store: &mut S) -> anyhow::Result<()> {
		match self {
			Self::Deposit { .. } => store.save_to_array(self).await?,
			Self::Broadcast { .. } => store.save_singleton(self).await?,
			Self::VaultDeposit { .. } => store.save_singleton(self).await?,
		};
		Ok(())
	}
}

impl Storable for WitnessInformation {
	fn get_key(&self) -> anyhow::Result<String> {
		let chain = self.to_foreign_chain().to_string();

		match self {
			Self::Deposit { deposit_address, .. } =>
				Ok(format!("deposit:{chain}:{deposit_address}")),
			Self::Broadcast { broadcast_id, .. } => Ok(format!("broadcast:{chain}:{broadcast_id}")),
			Self::VaultDeposit { tx_id, .. } => Ok(format!("vault_deposit:{chain}:{tx_id}")),
		}
	}

	fn get_expiry_duration(&self) -> std::time::Duration {
		match self.to_foreign_chain() {
			ForeignChain::Bitcoin => std::time::Duration::from_secs(3600 * 6),
			_ => Self::DEFAULT_EXPIRY_DURATION,
		}
	}
}

trait IntoDepositDetailsAnyChain {
	fn into_any_chain(self) -> Option<DepositDetails>;
}

impl IntoDepositDetailsAnyChain for cf_chains::evm::DepositDetails {
	fn into_any_chain(self) -> Option<DepositDetails> {
		self.tx_hashes.map(|tx_hashes| DepositDetails::Ethereum { tx_hashes })
	}
}
impl IntoDepositDetailsAnyChain for cf_chains::btc::Utxo {
	fn into_any_chain(self) -> Option<DepositDetails> {
		Some(DepositDetails::Bitcoin { tx_id: self.id.tx_id, vout: self.id.vout })
	}
}
impl IntoDepositDetailsAnyChain for u32 {
	fn into_any_chain(self) -> Option<DepositDetails> {
		Some(DepositDetails::Polkadot { extrinsic_index: self })
	}
}
impl IntoDepositDetailsAnyChain for Vec<H256> {
	fn into_any_chain(self) -> Option<DepositDetails> {
		Some(DepositDetails::Arbitrum { tx_hashes: self })
	}
}
impl IntoDepositDetailsAnyChain for () {
	fn into_any_chain(self) -> Option<DepositDetails> {
		None
	}
}

trait DepositWitnessExt<C: Chain> {
	fn into_witness_information(
		self,
		height: <C as Chain>::ChainBlockNumber,
		network: NetworkEnvironment,
	) -> WitnessInformation;
}

impl<C: Chain + Get<ForeignChain>> DepositWitnessExt<C> for DepositWitness<C>
where
	C::DepositDetails: IntoDepositDetailsAnyChain,
{
	fn into_witness_information(
		self,
		height: C::ChainBlockNumber,
		network: NetworkEnvironment,
	) -> WitnessInformation {
		WitnessInformation::Deposit {
			deposit_chain_block_height: height.into(),
			deposit_address: match C::get() {
				// TODO: consider using the humanreadable impl for Polkadot (ss58)
				ForeignChain::Polkadot => hex_encode_bytes(&self.deposit_address.encode()),
				_ => format!(
					"{}",
					self.deposit_address.into_foreign_chain_address().to_humanreadable(network)
				),
			},
			amount: self.amount.into().into(),
			asset: self.asset.into(),
			deposit_details: self.deposit_details.into_any_chain(),
		}
	}
}

trait VaultDepositWitnessExt<C: Chain> {
	fn into_witness_information(
		self,
		height: <C as Chain>::ChainBlockNumber,
		affiliate_mapping: &BTreeMap<AffiliateShortId, AccountId32>,
	) -> WitnessInformation;
}

impl<T: pallet_cf_ingress_egress::Config<I, AccountId = AccountId32>, I: 'static>
	VaultDepositWitnessExt<T::TargetChain> for VaultDepositWitness<T, I>
where
	<T::TargetChain as Chain>::DepositDetails: IntoDepositDetailsAnyChain,
{
	fn into_witness_information(
		self,
		height: <T::TargetChain as Chain>::ChainBlockNumber,
		affiliate_mapping: &BTreeMap<AffiliateShortId, AccountId32>,
	) -> WitnessInformation {
		WitnessInformation::VaultDeposit {
			tx_id: <<<T::TargetChain as Chain>::ChainCrypto as ChainCrypto>::TransactionInId as IntoTransactionInIdForAnyChain<
				<T::TargetChain as Chain>::ChainCrypto,
			>>::into_transaction_in_id_for_any_chain(self.tx_id),
			deposit_chain_block_height: height.into(),
			input_asset: self.input_asset.into(),
			output_asset: self.output_asset,
			amount: self.deposit_amount.into().into(),
			destination_address: format!("{}", self.destination_address),
			ccm_deposit_metadata: self.deposit_metadata,
			deposit_details: self.deposit_details.into_any_chain(),
			broker_fee: self.broker_fee.clone(),
			affiliate_fees: self.affiliate_fees
				.into_iter()
				.map(|affiliate| {
					Beneficiary { account: affiliate_mapping.get(&affiliate.account).cloned()
						.unwrap_or_else(|| {
							log::warn!(
								"Affiliate not found for short id {} on broker `{}`",
								affiliate.account,
								self.broker_fee.account,
							);
							AccountId32::from([0; 32])
						}), bps: affiliate.bps }
				})
				.collect::<Vec<Beneficiary<AccountId32>>>()
				.try_into()
				.expect("We collect into the same Affiliates type we started with, so the Vec bound is the same."),
					refund_params: self.refund_params,
					dca_params: self.dca_params,
					boost_fee: self.boost_fee,
				}
	}
}

trait BroadcastIntoWitnessInformation {
	fn into_witness_information(self) -> anyhow::Result<WitnessInformation>;
}

impl BroadcastIntoWitnessInformation for BroadcastDetails<Ethereum> {
	fn into_witness_information(self) -> anyhow::Result<WitnessInformation> {
		Ok(WitnessInformation::Broadcast {
			broadcast_id: self.broadcast_id,
			tx_out_id: TransactionId::Ethereum { signature: self.tx_out_id },
			tx_ref: TransactionRef::Ethereum { hash: self.tx_ref },
		})
	}
}

impl BroadcastIntoWitnessInformation for BroadcastDetails<Arbitrum> {
	fn into_witness_information(self) -> anyhow::Result<WitnessInformation> {
		Ok(WitnessInformation::Broadcast {
			broadcast_id: self.broadcast_id,
			tx_out_id: TransactionId::Arbitrum { signature: self.tx_out_id },
			tx_ref: TransactionRef::Arbitrum { hash: self.tx_ref },
		})
	}
}

impl BroadcastIntoWitnessInformation for BroadcastDetails<Bitcoin> {
	fn into_witness_information(self) -> anyhow::Result<WitnessInformation> {
		Ok(WitnessInformation::Broadcast {
			broadcast_id: self.broadcast_id,
			tx_out_id: TransactionId::Bitcoin { hash: BitcoinHash(self.tx_out_id) },
			tx_ref: TransactionRef::Bitcoin { hash: BitcoinHash(self.tx_ref) },
		})
	}
}

impl BroadcastIntoWitnessInformation for BroadcastDetails<Polkadot> {
	fn into_witness_information(self) -> anyhow::Result<WitnessInformation> {
		Ok(WitnessInformation::Broadcast {
			broadcast_id: self.broadcast_id,
			tx_out_id: TransactionId::Polkadot {
				signature: DotSignature(*self.tx_out_id.aliased_ref()),
			},
			tx_ref: TransactionRef::Polkadot { transaction_id: self.tx_ref },
		})
	}
}

async fn save_deposit_witnesses<S, C>(
	store: &mut S,
	deposit_witnesses: Vec<DepositWitness<C>>,
	block_height: C::ChainBlockNumber,
	network: NetworkEnvironment,
) where
	C::DepositDetails: IntoDepositDetailsAnyChain,
	S: Store,
	C: Chain + Get<ForeignChain>,
{
	for witness in deposit_witnesses {
		let _ = witness
			.into_witness_information(block_height, network)
			.save_to_store(store)
			.await
			.inspect_err(|e| {
				tracing::error!("Failed to save deposit witness: {:?}", e);
			});
	}
}

#[cfg_attr(test, mockall::automock)]
pub trait CfGetAffiliates {
	async fn cf_get_affiliates(
		&self,
		broker_fee_account: &AccountId32,
	) -> anyhow::Result<BTreeMap<AffiliateShortId, state_chain_runtime::AccountId>>;
}

impl<S> CfGetAffiliates for StateChainClient<S> {
	async fn cf_get_affiliates(
		&self,
		broker_fee_account: &AccountId32,
	) -> anyhow::Result<BTreeMap<AffiliateShortId, state_chain_runtime::AccountId>> {
		use custom_rpc::CustomApiClient;

		Ok(self
			.base_rpc_client
			.raw_rpc_client
			.cf_get_affiliates(broker_fee_account.clone(), None)
			.await?
			.into_iter()
			.collect::<BTreeMap<_, _>>())
	}
}

async fn save_vault_deposit_witness<S, T, I: 'static, StateChainClient>(
	store: &mut S,
	deposit_witness: VaultDepositWitness<T, I>,
	block_height: <T::TargetChain as Chain>::ChainBlockNumber,
	state_chain_client: Arc<StateChainClient>,
) -> anyhow::Result<()>
where
	S: Store,
	T: pallet_cf_ingress_egress::Config<I, AccountId = AccountId32>,
	<T::TargetChain as Chain>::DepositDetails: IntoDepositDetailsAnyChain,
	StateChainClient: CfGetAffiliates + 'static + Send + Sync,
{
	let affiliate_mapping = state_chain_client
		.cf_get_affiliates(&deposit_witness.broker_fee.account)
		.await?;

	let _ = deposit_witness
		.into_witness_information(block_height, &affiliate_mapping)
		.save_to_store(store)
		.await
		.inspect_err(|e| {
			tracing::error!("Failed to save vault deposit witness: {:?}", e);
		});
	Ok(())
}

async fn save_broadcast_witness<S: Store, StateChainClient, I>(
	store: &mut S,
	tx_out_id: TransactionOutIdFor<state_chain_runtime::Runtime, ChainInstanceFor<I>>,
	tx_ref: I::TransactionRef,
	state_chain_client: Arc<StateChainClient>,
) -> anyhow::Result<()>
where
	I: cf_chains::instances::ChainInstanceAlias + Chain + 'static,
	StateChainClient: StorageApi + ChainApi + 'static + Send + Sync,
	state_chain_runtime::Runtime: pallet_cf_broadcast::Config<ChainInstanceFor<I>>,
	BroadcastDetails<I>: BroadcastIntoWitnessInformation,
{
	if let Some(broadcast_details) =
		get_broadcast_id::<I, StateChainClient>(state_chain_client, &tx_out_id)
			.await
			.map(|broadcast_id| BroadcastDetails::<I> { broadcast_id, tx_out_id, tx_ref })
	{
		match broadcast_details.into_witness_information() {
			// In case of errors, log it and continue operation
			Ok(witness) => {
				let _result = witness.save_to_store(store).await.map_err(|e| {
					tracing::error!("Failed to save broadcast witness: {:?}", e);
					e
				});
			},
			Err(e) => {
				tracing::error!("Failed to save broadcast witness: {:?}", e);
			},
		};
	}
	Ok(())
}

pub async fn handle_call<S, StateChainClient>(
	call: state_chain_runtime::RuntimeCall,
	store: &mut S,
	chainflip_network: NetworkEnvironment,
	state_chain_client: Arc<StateChainClient>,
) -> anyhow::Result<()>
where
	S: Store,
	StateChainClient: StorageApi + CfGetAffiliates + ChainApi + 'static + Send + Sync,
{
	use pallet_cf_broadcast::Call as BroadcastCall;
	use pallet_cf_ingress_egress::Call as IngressEgressCall;
	use state_chain_runtime::RuntimeCall::*;

	match call {
		EthereumIngressEgress(IngressEgressCall::process_deposits {
			deposit_witnesses,
			block_height,
		}) => save_deposit_witnesses(store, deposit_witnesses, block_height, chainflip_network).await,
		BitcoinIngressEgress(IngressEgressCall::process_deposits {
			deposit_witnesses,
			block_height,
		}) => save_deposit_witnesses(store, deposit_witnesses, block_height, chainflip_network).await,
		PolkadotIngressEgress(IngressEgressCall::process_deposits {
			deposit_witnesses,
			block_height,
		}) => save_deposit_witnesses(store, deposit_witnesses, block_height, chainflip_network).await,
		ArbitrumIngressEgress(IngressEgressCall::process_deposits {
			deposit_witnesses,
			block_height,
		}) => save_deposit_witnesses(store, deposit_witnesses, block_height, chainflip_network).await,
		SolanaIngressEgress(IngressEgressCall::process_deposits {
			deposit_witnesses,
			block_height,
		}) => save_deposit_witnesses(store, deposit_witnesses, block_height, chainflip_network).await,
		EthereumIngressEgress(IngressEgressCall::vault_swap_request { block_height, deposit }) =>
			save_vault_deposit_witness(store, *deposit, block_height, state_chain_client).await?,
		BitcoinIngressEgress(IngressEgressCall::vault_swap_request { block_height, deposit }) =>
			save_vault_deposit_witness(store, *deposit, block_height, state_chain_client).await?,
		PolkadotIngressEgress(IngressEgressCall::vault_swap_request { block_height, deposit }) =>
			save_vault_deposit_witness(store, *deposit, block_height, state_chain_client).await?,
		ArbitrumIngressEgress(IngressEgressCall::vault_swap_request { block_height, deposit }) =>
			save_vault_deposit_witness(store, *deposit, block_height, state_chain_client).await?,
		SolanaIngressEgress(IngressEgressCall::vault_swap_request { block_height, deposit }) =>
			save_vault_deposit_witness(store, *deposit, block_height, state_chain_client).await?,
		EthereumBroadcaster(BroadcastCall::transaction_succeeded {
			tx_out_id,
			transaction_ref,
			..
		}) => {
			save_broadcast_witness::<_, _, Ethereum>(
				store,
				tx_out_id,
				transaction_ref,
				state_chain_client,
			)
			.await?;
		},
		BitcoinBroadcaster(BroadcastCall::transaction_succeeded {
			tx_out_id,
			transaction_ref,
			..
		}) => {
			save_broadcast_witness::<_, _, Bitcoin>(
				store,
				tx_out_id,
				transaction_ref,
				state_chain_client,
			)
			.await?;
		},
		PolkadotBroadcaster(BroadcastCall::transaction_succeeded {
			tx_out_id,
			transaction_ref,
			..
		}) => {
			save_broadcast_witness::<_, _, Polkadot>(
				store,
				tx_out_id,
				transaction_ref,
				state_chain_client,
			)
			.await?;
		},
		ArbitrumBroadcaster(BroadcastCall::transaction_succeeded {
			tx_out_id,
			transaction_ref,
			..
		}) => {
			save_broadcast_witness::<_, _, Arbitrum>(
				store,
				tx_out_id,
				transaction_ref,
				state_chain_client,
			)
			.await?;
		},
		SolanaBroadcaster(BroadcastCall::transaction_succeeded {
			tx_out_id: _,
			transaction_ref: _,
			..
		}) => todo!(),

		EthereumIngressEgress(_) |
		BitcoinIngressEgress(_) |
		PolkadotIngressEgress(_) |
		ArbitrumIngressEgress(_) |
		SolanaIngressEgress(_) |
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
		ArbitrumChainTracking(_) |
		SolanaChainTracking(_) |
		EthereumVault(_) |
		PolkadotVault(_) |
		BitcoinVault(_) |
		ArbitrumVault(_) |
		SolanaVault(_) |
		EvmThresholdSigner(_) |
		PolkadotThresholdSigner(_) |
		BitcoinThresholdSigner(_) |
		SolanaThresholdSigner(_) |
		EthereumBroadcaster(_) |
		PolkadotBroadcaster(_) |
		BitcoinBroadcaster(_) |
		ArbitrumBroadcaster(_) |
		SolanaBroadcaster(_) |
		Swapping(_) |
		LiquidityProvider(_) |
		LiquidityPools(_) |
		SolanaElections(_) => {},
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
		instances::ChainInstanceFor,
		Chain, ForeignChainAddress,
	};
	use cf_utilities::assert_ok;
	use chainflip_api::primitives::AffiliateShortId;
	use chainflip_engine::state_chain_observer::client::{mocks::MockStateChainClient, BlockInfo};
	use pallet_cf_ingress_egress::DepositWitness;
	use sp_core::H160;
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
			let key = storable.get_key()?;
			let value = serde_json::to_value(storable)?;

			let array = self.storage.entry(key).or_insert(serde_json::Value::Array(vec![]));

			array.as_array_mut().ok_or(anyhow!("expect array"))?.push(value);

			Ok(())
		}

		async fn save_singleton<S: Storable>(
			&mut self,
			storable: &S,
		) -> anyhow::Result<Self::Output> {
			let key = storable.get_key()?;

			let value = serde_json::to_value(storable)?;

			self.storage.insert(key, value);

			Ok(())
		}
	}

	// This implementation is necessary to keep the compiler happy.
	impl CfGetAffiliates for MockStateChainClient {
		async fn cf_get_affiliates(
			&self,
			_broker_fee_account: &AccountId32,
		) -> anyhow::Result<BTreeMap<AffiliateShortId, state_chain_runtime::AccountId>> {
			unimplemented!(
				"This is not tested via the MockStateChainClient, use MockCfGetAffiliates instead."
			)
		}
	}

	#[allow(clippy::type_complexity)]
	fn create_client<C: Chain>(
		result: Option<(BroadcastId, C::ChainBlockNumber)>,
	) -> MockStateChainClient
	where
		state_chain_runtime::Runtime:
			pallet_cf_broadcast::Config<ChainInstanceFor<C>, TargetChain = C>,
	{
		let mut state_chain_client = MockStateChainClient::new();

		state_chain_client
			.expect_storage_map_entry::<pallet_cf_broadcast::TransactionOutIdToBroadcastId<
				state_chain_runtime::Runtime,
				ChainInstanceFor<C>,
			>>()
			.return_once(move |_, _| Ok(result));

		state_chain_client.expect_latest_unfinalized_block().returning(|| BlockInfo {
			parent_hash: state_chain_runtime::Hash::default(),
			hash: state_chain_runtime::Hash::default(),
			number: 1,
		});

		state_chain_client
	}

	fn parse_eth_address(address: &str) -> (H160, &str) {
		let eth_address_bytes = H160::from_slice(&hex::decode(&address[2..]).unwrap());
		(eth_address_bytes, address)
	}

	#[tokio::test]
	async fn test_handle_deposit_calls() {
		let polkadot_address = "14JWPRWMkEyLLdrN2k3teBd446sKKJuwU2DDKw4Ev4dYcHeF";
		let polkadot_account_id = polkadot_address.parse::<PolkadotAccountId>().unwrap();

		let (eth_address1, eth_address_str1) =
			parse_eth_address("0x541f563237A309B3A61E33BDf07a8930Bdba8D99");

		let (eth_address2, eth_address_str2) =
			parse_eth_address("0xa56A6be23b6Cf39D9448FF6e897C29c41c8fbDFF");

		let client = Arc::new(MockStateChainClient::new());

		let mut store = MockStore::default();
		assert_ok!(
			handle_call(
				state_chain_runtime::RuntimeCall::EthereumIngressEgress(
					pallet_cf_ingress_egress::Call::process_deposits {
						deposit_witnesses: vec![DepositWitness {
							deposit_address: eth_address1,
							amount: 100u128,
							asset: cf_chains::assets::eth::Asset::Eth,
							deposit_details: Default::default(),
						}],
						block_height: 1,
					},
				),
				&mut store,
				NetworkEnvironment::Testnet,
				client.clone(),
			)
			.await
		);

		assert_ok!(
			handle_call(
				state_chain_runtime::RuntimeCall::PolkadotIngressEgress(
					pallet_cf_ingress_egress::Call::process_deposits {
						deposit_witnesses: vec![DepositWitness {
							deposit_address: polkadot_account_id,
							amount: 100u128,
							asset: cf_chains::assets::dot::Asset::Dot,
							deposit_details: 1,
						}],
						block_height: 1,
					},
				),
				&mut store,
				NetworkEnvironment::Testnet,
				client.clone(),
			)
			.await
		);

		assert_ok!(
			handle_call(
				state_chain_runtime::RuntimeCall::EthereumIngressEgress(
					pallet_cf_ingress_egress::Call::process_deposits {
						deposit_witnesses: vec![DepositWitness {
							deposit_address: eth_address2,
							amount: 100u128,
							asset: cf_chains::assets::eth::Asset::Eth,
							deposit_details: Default::default(),
						}],
						block_height: 1,
					},
				),
				&mut store,
				NetworkEnvironment::Testnet,
				client.clone(),
			)
			.await
		);

		assert_eq!(store.storage.len(), 3);
		println!("{:?}", store.storage);
		insta::assert_snapshot!(store
			.storage
			.get(format!("deposit:Ethereum:{}", eth_address_str1.to_lowercase()).as_str())
			.unwrap());
		insta::assert_snapshot!(store
			.storage
			.get(
				format!("deposit:Polkadot:0x{}", hex::encode(polkadot_account_id.aliased_ref()))
					.as_str()
			)
			.unwrap());
		insta::assert_snapshot!(store
			.storage
			.get(format!("deposit:Ethereum:{}", eth_address_str2.to_lowercase()).as_str())
			.unwrap());

		assert_ok!(
			handle_call(
				state_chain_runtime::RuntimeCall::EthereumIngressEgress(
					pallet_cf_ingress_egress::Call::process_deposits {
						deposit_witnesses: vec![DepositWitness {
							deposit_address: eth_address1,
							amount: 2_000_000u128,
							asset: cf_chains::assets::eth::Asset::Eth,
							deposit_details: Default::default(),
						}],
						block_height: 1,
					},
				),
				&mut store,
				NetworkEnvironment::Testnet,
				client,
			)
			.await
		);
		assert_eq!(store.storage.len(), 3);
		insta::assert_snapshot!(store
			.storage
			.get(format!("deposit:Ethereum:{}", eth_address_str1.to_lowercase()).as_str())
			.unwrap());
	}

	#[tokio::test]
	async fn test_handle_broadcast_calls() {
		let (eth_address, _) = parse_eth_address("0x541f563237A309B3A61E33BDf07a8930Bdba8D99");

		let tx_out_id = SchnorrVerificationComponents { s: [0; 32], k_times_g_address: [0; 20] };

		let client = Arc::new(create_client::<Ethereum>(Some((1, 2))));
		let mut store = MockStore::default();
		assert_ok!(
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
						transaction_ref: Default::default(),
					},
				),
				&mut store,
				NetworkEnvironment::Testnet,
				client,
			)
			.await
		);

		assert_eq!(store.storage.len(), 1);
		insta::assert_snapshot!(store.storage.get("broadcast:Ethereum:1").unwrap());
	}

	#[tokio::test]
	async fn test_handle_vault_deposit_calls() {
		chainflip_api::use_chainflip_account_id_encoding();
		let (eth_address, _) = parse_eth_address("0x541f563237A309B3A61E33BDf07a8930Bdba8D99");
		let affiliate_short_id = AffiliateShortId::from(69);

		let tx_id = H256::from_slice(
			&hex::decode("b5c8bd9430b6cc87a0e2fe110ece6bf527fa4f170a4bc8cd032f768fc5219838")
				.unwrap(),
		);

		let mut client = MockCfGetAffiliates::new();

		// We expect the affiliates to be mapped from short id's to account id's
		client.expect_cf_get_affiliates().returning(move |_| {
			Ok(BTreeMap::from_iter([(affiliate_short_id, AccountId32::new([1; 32]))]))
		});

		let client = Arc::new(client);

		let mut store = MockStore::default();
		assert_ok!(
			save_vault_deposit_witness(
				&mut store,
				VaultDepositWitness::<
					state_chain_runtime::Runtime,
					state_chain_runtime::EthereumInstance,
				> {
					tx_id,
					deposit_address: Some(eth_address),
					channel_id: Default::default(),
					deposit_amount: 100u128,
					input_asset: cf_chains::assets::eth::Asset::Eth,
					output_asset: chainflip_api::primitives::Asset::Flip,
					destination_address: cf_chains::address::EncodedAddress::Dot([0; 32]),
					deposit_metadata: None,
					deposit_details: Default::default(),
					broker_fee: Beneficiary { account: AccountId32::new([0; 32]), bps: 10 },
					affiliate_fees: frame_support::BoundedVec::try_from(vec![Beneficiary {
						account: affiliate_short_id,
						bps: 10
					}])
					.unwrap(),
					refund_params: ChannelRefundParameters {
						refund_address: ForeignChainAddress::Eth(eth_address),
						retry_duration: Default::default(),
						min_price: Default::default(),
					},
					dca_params: Some(DcaParameters { number_of_chunks: 5, chunk_interval: 100 }),
					boost_fee: 5,
				},
				1,
				client
			)
			.await
		);

		assert_eq!(store.storage.len(), 1);
		insta::assert_snapshot!(store
			.storage
			.get("vault_deposit:Ethereum:b5c8bd9430b6cc87a0e2fe110ece6bf527fa4f170a4bc8cd032f768fc5219838")
			.unwrap());
	}

	#[test]
	fn serialization_works_as_expected() {
		let h = BitcoinHash(
			[
				0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
				23, 24, 25, 26, 27, 28, 29, 30, 31,
			]
			.into(),
		);
		let s = DotSignature([
			0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
			24, 25, 26, 27, 28, 29, 30, 31, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
			16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
		]);

		assert_eq!(
			serde_json::to_string(&h).unwrap(),
			"\"0x1f1e1d1c1b1a191817161514131211100f0e0d0c0b0a09080706050403020100\""
		);
		assert_eq!(serde_json::to_string(&s).unwrap(), "\"0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f\"");
	}
}
