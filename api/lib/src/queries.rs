use super::*;
use cf_chains::{address::ToHumanreadableAddress, Chain};
use cf_primitives::{chains::assets::any, AssetAmount, FlipBalance};
use chainflip_engine::state_chain_observer::client::{
	chain_api::ChainApi, storage_api::StorageApi,
};
use codec::Decode;
use custom_rpc::CustomApiClient;
use frame_support::sp_runtime::DigestItem;
use pallet_cf_ingress_egress::DepositChannelDetails;
use pallet_cf_validator::RotationPhase;
use serde::Deserialize;
use sp_consensus_aura::{Slot, AURA_ENGINE_ID};
use state_chain_runtime::{runtime_apis::FailingWitnessValidators, PalletInstanceAlias};
use std::{collections::BTreeMap, ops::Deref, sync::Arc};
use tracing::log;
use utilities::task_scope;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapChannelInfo<C: Chain> {
	deposit_address: <C::ChainAccount as ToHumanreadableAddress>::Humanreadable,
	source_asset: any::OldAsset,
	destination_asset: any::OldAsset,
}

pub struct PreUpdateStatus {
	pub rotation: bool,
	pub is_authority: bool,
	pub next_block_in: Option<usize>,
}

pub struct QueryApi {
	pub(crate) state_chain_client: Arc<StateChainClient>,
}

impl QueryApi {
	pub async fn connect<'a>(
		scope: &task_scope::Scope<'a, anyhow::Error>,
		state_chain_settings: &settings::StateChain,
	) -> Result<QueryApi> {
		log::debug!("Connecting to state chain at: {}", state_chain_settings.ws_endpoint);

		let (.., state_chain_client) = StateChainClient::connect_with_account(
			scope,
			&state_chain_settings.ws_endpoint,
			&state_chain_settings.signing_key_file,
			AccountRole::Unregistered,
			false,
			false,
			false,
		)
		.await?;

		Ok(Self { state_chain_client })
	}

	pub async fn get_open_swap_channels<C: Chain + PalletInstanceAlias>(
		&self,
		block_hash: Option<state_chain_runtime::Hash>,
	) -> Result<Vec<SwapChannelInfo<C>>, anyhow::Error>
	where
		state_chain_runtime::Runtime:
			pallet_cf_ingress_egress::Config<C::Instance, TargetChain = C>,
	{
		let block_hash =
			block_hash.unwrap_or_else(|| self.state_chain_client.latest_finalized_block().hash);

		let (channel_details, network_environment) = tokio::try_join!(
			self.state_chain_client
				.storage_map::<pallet_cf_ingress_egress::DepositChannelLookup<
					state_chain_runtime::Runtime,
					C::Instance,
				>, Vec<_>>(block_hash)
				.map(|result| {
					result.map(|channels| channels.into_iter().collect::<BTreeMap<_, _>>())
				}),
			self.state_chain_client
				.storage_value::<pallet_cf_environment::ChainflipNetworkEnvironment<state_chain_runtime::Runtime>>(
					block_hash
				),
		)?;

		Ok(channel_details
			.iter()
			.filter_map(|(_, DepositChannelDetails { action, deposit_channel, .. })| match action {
				pallet_cf_ingress_egress::ChannelAction::Swap { destination_asset, .. } |
				pallet_cf_ingress_egress::ChannelAction::CcmTransfer {
					destination_asset, ..
				} => Some(SwapChannelInfo {
					deposit_address: deposit_channel.address.to_humanreadable(network_environment),
					source_asset: Into::<Asset>::into(deposit_channel.asset).into(),
					destination_asset: (*destination_asset).into(),
				}),
				_ => None,
			})
			.collect::<Vec<_>>())
	}

	pub async fn get_balances(
		&self,
		block_hash: Option<state_chain_runtime::Hash>,
	) -> Result<BTreeMap<Asset, AssetAmount>> {
		let block_hash =
			block_hash.unwrap_or_else(|| self.state_chain_client.latest_finalized_block().hash);

		futures::future::join_all(Asset::all().map(|asset| async move {
			Ok((
				asset,
				self.state_chain_client
					.storage_double_map_entry::<pallet_cf_lp::FreeBalances<state_chain_runtime::Runtime>>(
						block_hash,
						&self.state_chain_client.account_id(),
						&asset,
					)
					.await?
					.unwrap_or_default(),
			))
		}))
		.await
		.into_iter()
		.collect()
	}

	pub async fn get_bound_redeem_address(
		&self,
		block_hash: Option<state_chain_runtime::Hash>,
		account_id: Option<state_chain_runtime::AccountId>,
	) -> Result<Option<EthereumAddress>, anyhow::Error> {
		let block_hash =
			block_hash.unwrap_or_else(|| self.state_chain_client.latest_finalized_block().hash);
		let account_id = account_id.unwrap_or_else(|| self.state_chain_client.account_id());

		Ok(self
			.state_chain_client
			.storage_map_entry::<pallet_cf_funding::BoundRedeemAddress<state_chain_runtime::Runtime>>(
				block_hash,
				&account_id,
			)
			.await?)
	}

	pub async fn get_bound_executor_address(
		&self,
		block_hash: Option<state_chain_runtime::Hash>,
		account_id: Option<state_chain_runtime::AccountId>,
	) -> Result<Option<EthereumAddress>, anyhow::Error> {
		let block_hash =
			block_hash.unwrap_or_else(|| self.state_chain_client.latest_finalized_block().hash);
		let account_id = account_id.unwrap_or_else(|| self.state_chain_client.account_id());

		Ok(self
			.state_chain_client
			.storage_map_entry::<pallet_cf_funding::BoundExecutorAddress<state_chain_runtime::Runtime>>(
				block_hash,
				&account_id,
			)
			.await?)
	}

	pub async fn get_restricted_balances(
		&self,
		block_hash: Option<state_chain_runtime::Hash>,
		account_id: Option<state_chain_runtime::AccountId>,
	) -> Result<BTreeMap<EthereumAddress, FlipBalance>> {
		let block_hash =
			block_hash.unwrap_or_else(|| self.state_chain_client.latest_finalized_block().hash);
		let account_id = account_id.unwrap_or_else(|| self.state_chain_client.account_id());

		Ok(self
			.state_chain_client
			.storage_map_entry::<pallet_cf_funding::RestrictedBalances<state_chain_runtime::Runtime>>(
				block_hash,
				&account_id,
			)
			.await?)
	}

	pub async fn pre_update_check(
		&self,
		block_hash: Option<state_chain_runtime::Hash>,
		account_id: Option<state_chain_runtime::AccountId>,
	) -> Result<PreUpdateStatus, anyhow::Error> {
		let block_hash =
			block_hash.unwrap_or_else(|| self.state_chain_client.latest_finalized_block().hash);
		let account_id = account_id.unwrap_or_else(|| self.state_chain_client.account_id());

		let mut result =
			PreUpdateStatus { rotation: false, is_authority: false, next_block_in: None };

		if self
			.state_chain_client
			.storage_value::<pallet_cf_validator::CurrentRotationPhase<state_chain_runtime::Runtime>>(
				block_hash,
			)
			.await? != RotationPhase::Idle
		{
			result.rotation = true;
		}

		let current_validators = self
			.state_chain_client
			.storage_value::<pallet_cf_validator::CurrentAuthorities<state_chain_runtime::Runtime>>(
				block_hash,
			)
			.await?;

		if current_validators.contains(&account_id) {
			result.is_authority = true;
		} else {
			return Ok(result)
		}

		let header = self.state_chain_client.base_rpc_client.block_header(block_hash).await?;

		let slot: usize =
			*extract_slot_from_digest_item(&header.digest.logs[0]).unwrap().deref() as usize;

		let validator_len = current_validators.len();
		let current_relative_slot = slot % validator_len;
		let index = current_validators.iter().position(|account| account == &account_id).unwrap();

		result.next_block_in = Some(compute_distance(index, current_relative_slot, validator_len));
		Ok(result)
	}

	pub async fn check_witnesses(
		&self,
		block_hash: Option<state_chain_runtime::Hash>,
		hash: state_chain_runtime::Hash,
	) -> Result<Option<FailingWitnessValidators>, anyhow::Error> {
		let result = self
			.state_chain_client
			.base_rpc_client
			.raw_rpc_client
			.cf_witness_count(hash, block_hash)
			.await?;

		Ok(result)
	}
}

// https://github.com/chainflip-io/substrate/blob/c172d0f683fab3792b90d876fd6ca27056af9fe9/frame/aura/src/lib.rs#L179
fn extract_slot_from_digest_item(item: &DigestItem) -> Option<Slot> {
	item.as_pre_runtime().and_then(|(id, mut data)| {
		if id == AURA_ENGINE_ID {
			Slot::decode(&mut data).ok()
		} else {
			None
		}
	})
}

fn compute_distance(index: usize, slot: usize, len: usize) -> usize {
	if index >= slot {
		index - slot
	} else {
		len - slot + index
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use codec::Encode;

	#[test]
	fn test_slot_extraction() {
		let slot = Slot::from(42);
		assert_eq!(
			Some(slot),
			extract_slot_from_digest_item(&DigestItem::PreRuntime(
				AURA_ENGINE_ID,
				Encode::encode(&slot)
			))
		);
		assert_eq!(
			None,
			extract_slot_from_digest_item(&DigestItem::PreRuntime(*b"BORA", Encode::encode(&slot)))
		);
		assert_eq!(
			None,
			extract_slot_from_digest_item(&DigestItem::Other(b"SomethingElse".to_vec()))
		);
	}

	#[test]
	fn test_compute_distance() {
		let index: usize = 5;
		let slot: usize = 7;
		let len: usize = 15;

		assert_eq!(compute_distance(index, slot, len), 13);

		let index: usize = 18;
		let slot: usize = 7;
		let len: usize = 24;

		assert_eq!(compute_distance(index, slot, len), 11);
	}
}
