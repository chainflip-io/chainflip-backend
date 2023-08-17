use std::sync::Arc;

use cf_chains::Ethereum;
use ethers::{prelude::abigen, types::Bloom};
use sp_core::{H160, H256};
use tracing::{info, trace};

use crate::{
	eth::retry_rpc::EthersRetryRpcApi,
	state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi,
};

use super::{
	super::common::{
		chain_source::ChainClient,
		chunked_chain_source::chunked_by_vault::{builder::ChunkedByVaultBuilder, ChunkedByVault},
	},
	contract_common::events_at_block,
};

abigen!(
	StateChainGateway,
	"$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IStateChainGateway.json"
);

use anyhow::Result;

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub fn state_chain_gateway_witnessing<
		StateChainClient,
		EthRpcClient: EthersRetryRpcApi + ChainClient + Clone,
	>(
		self,
		state_chain_client: Arc<StateChainClient>,
		eth_rpc: EthRpcClient,
		contract_address: H160,
	) -> ChunkedByVaultBuilder<impl ChunkedByVault>
	where
		Inner: ChunkedByVault<Index = u64, Hash = H256, Data = Bloom, Chain = Ethereum>,
		StateChainClient: SignedExtrinsicApi + Send + Sync + 'static,
	{
		self.then::<Result<Bloom>, _, _>(move |epoch, header| {
			let state_chain_client = state_chain_client.clone();
			let eth_rpc = eth_rpc.clone();
			async move {
				for event in events_at_block::<StateChainGatewayEvents, _>(
					header,
					contract_address,
					&eth_rpc,
				)
				.await?
				{
					info!("Handling event: {event}");
					match event.event_parameters {
						StateChainGatewayEvents::FundedFilter(FundedFilter {
							node_id: account_id,
							amount,
							funder,
						}) => {
							state_chain_client
								.submit_signed_extrinsic(
									pallet_cf_witnesser::Call::witness_at_epoch {
										call: Box::new(
											pallet_cf_funding::Call::funded {
												account_id: account_id.into(),
												amount: amount
													.try_into()
													.expect("Funded amount should fit in u128"),
												funder,
												tx_hash: event.tx_hash.into(),
											}
											.into(),
										),
										epoch_index: epoch.index,
									},
								)
								.await;
						},
						StateChainGatewayEvents::RedemptionExecutedFilter(
							RedemptionExecutedFilter { node_id: account_id, amount },
						) => {
							state_chain_client
								.submit_signed_extrinsic(
									pallet_cf_witnesser::Call::witness_at_epoch {
										call: Box::new(
											pallet_cf_funding::Call::redeemed {
												account_id: account_id.into(),
												redeemed_amount: amount
													.try_into()
													.expect("Redemption amount should fit in u128"),
												tx_hash: event.tx_hash.to_fixed_bytes(),
											}
											.into(),
										),
										epoch_index: epoch.index,
									},
								)
								.await;
						},
						StateChainGatewayEvents::RedemptionExpiredFilter(
							RedemptionExpiredFilter { node_id: account_id, amount: _ },
						) => {
							state_chain_client
								.submit_signed_extrinsic(
									pallet_cf_witnesser::Call::witness_at_epoch {
										call: Box::new(
											pallet_cf_funding::Call::redemption_expired {
												account_id: account_id.into(),
												block_number: header.index,
											}
											.into(),
										),
										epoch_index: epoch.index,
									},
								)
								.await;
						},
						_ => {
							trace!("Ignoring unused event: {event}");
						},
					}
				}

				Result::Ok(header.data)
			}
		})
	}
}
