use cf_chains::Ethereum;
use ethers::{prelude::abigen, types::Bloom};
use sp_core::{H160, H256};
use tracing::{info, trace};

use super::super::{
	common::{
		chain_source::ChainClient,
		chunked_chain_source::chunked_by_vault::{builder::ChunkedByVaultBuilder, ChunkedByVault},
	},
	evm::contract_common::events_at_block,
};
use crate::evm::retry_rpc::EvmRetryRpcApi;
use cf_primitives::EpochIndex;
use futures_core::Future;

abigen!(
	StateChainGateway,
	"$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IStateChainGateway.json"
);

use anyhow::Result;

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub fn state_chain_gateway_witnessing<
		EvmRpcClient: EvmRetryRpcApi + ChainClient + Clone,
		ProcessCall,
		ProcessingFut,
	>(
		self,
		process_call: ProcessCall,
		eth_rpc: EvmRpcClient,
		contract_address: H160,
	) -> ChunkedByVaultBuilder<impl ChunkedByVault>
	where
		Inner: ChunkedByVault<Index = u64, Hash = H256, Data = Bloom, Chain = Ethereum>,
		ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
			+ Send
			+ Sync
			+ Clone
			+ 'static,
		ProcessingFut: Future<Output = ()> + Send + 'static,
	{
		self.then::<Result<Bloom>, _, _>(move |epoch, header| {
			let process_call = process_call.clone();
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
					let call: state_chain_runtime::RuntimeCall = match event.event_parameters {
						StateChainGatewayEvents::FundedFilter(FundedFilter {
							node_id: account_id,
							amount,
							funder,
						}) => pallet_cf_funding::Call::funded {
							account_id: account_id.into(),
							amount: amount.try_into().expect("Funded amount should fit in u128"),
							funder,
							tx_hash: event.tx_hash.into(),
						}
						.into(),
						StateChainGatewayEvents::RedemptionExecutedFilter(
							RedemptionExecutedFilter { node_id: account_id, amount },
						) => pallet_cf_funding::Call::redeemed {
							account_id: account_id.into(),
							redeemed_amount: amount
								.try_into()
								.expect("Redemption amount should fit in u128"),
							tx_hash: event.tx_hash.to_fixed_bytes(),
						}
						.into(),
						StateChainGatewayEvents::RedemptionExpiredFilter(
							RedemptionExpiredFilter { node_id: account_id, amount: _ },
						) => pallet_cf_funding::Call::redemption_expired {
							account_id: account_id.into(),
							block_number: header.index,
						}
						.into(),
						_ => {
							trace!("Ignoring unused event: {event}");
							continue
						},
					};
					process_call(call, epoch.index).await;
				}

				Result::Ok(header.data)
			}
		})
	}
}
