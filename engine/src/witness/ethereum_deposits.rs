use crate::state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi;
use cf_chains::Ethereum;
use cf_primitives::chains::assets::eth;
use ethers::types::Bloom;
use pallet_cf_ingress_egress::DepositChannelDetails;
use sp_core::H256;
use state_chain_runtime::EthereumInstance;
use std::sync::Arc;

use super::{
	address_checker::AddressCheckerApi,
	chunked_chain_source::chunked_by_vault::{builder::ChunkedByVaultBuilder, ChunkedByVault},
	eth_ingresses_at_block::eth_ingresses_at_block,
	vault::VaultApi,
};

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub fn ethereum_deposits<
		StateChainClient,
		AddressCheckerRpcClient: AddressCheckerApi + Send + Sync + Clone,
		VaultRpcClient: VaultApi + Send + Sync + Clone,
	>(
		self,
		state_chain_client: Arc<StateChainClient>,
		address_checker_rpc: AddressCheckerRpcClient,
		vault_rpc: VaultRpcClient,
	) -> ChunkedByVaultBuilder<impl ChunkedByVault>
	where
		Inner: ChunkedByVault<
			Index = u64,
			Hash = H256,
			Data = (Bloom, Vec<DepositChannelDetails<Ethereum>>),
			Chain = Ethereum,
		>,
		StateChainClient: SignedExtrinsicApi + Send + Sync + 'static,
	{
		self.then(move |epoch, header| {
			let address_checker_rpc = address_checker_rpc.clone();
			let vault_rpc = vault_rpc.clone();
			let state_chain_client = state_chain_client.clone();
			async move {
				let addresses = header
					.data
					.1
					.into_iter()
					.map(|address| address.deposit_channel.address)
					.collect::<Vec<_>>();

				let previous_block_balances = address_checker_rpc
					.balances(header.parent_hash.unwrap(), addresses.clone())
					.await
					.unwrap();

				let address_states = address_checker_rpc
					.address_states(header.hash, addresses.clone())
					.await
					.unwrap();

				let native_events = vault_rpc.fetched_native_events(header.hash).await.unwrap();

				let ingresses = eth_ingresses_at_block(
					addresses,
					previous_block_balances,
					address_states,
					native_events,
				);

				if !ingresses.is_empty() {
					state_chain_client
							.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
								call: Box::new(
									pallet_cf_ingress_egress::Call::<_, EthereumInstance>::process_deposits {
										deposit_witnesses: ingresses.into_iter().map(|(to_addr, value)| {
											pallet_cf_ingress_egress::DepositWitness {
												deposit_address: to_addr,
												asset: eth::Asset::Eth,
												amount:
													value
													.try_into()
													.expect("Ingress witness transfer value should fit u128"),
												deposit_details: (),
											}
										}).collect(),
										block_height: header.index,
									}
									.into(),
								),
								epoch_index: epoch.index,
							})
							.await;
				}
			}
		})
	}
}
