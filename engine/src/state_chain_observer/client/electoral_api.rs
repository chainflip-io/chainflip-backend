use crate::state_chain_observer::client::{
	base_rpc_api::RawRpcApi, extrinsic_api::signed::SignedExtrinsicApi, BaseRpcClient, BlockInfo,
	StateChainClient,
};
use codec::{Decode, Encode};
use pallet_cf_elections::{
	vote_storage::VoteStorage, ElectionIdentifierOf, ElectoralDataFor, ElectoralSystemRunner,
	VoteOf,
};
use state_chain_runtime::SolanaInstance;
use std::collections::{BTreeMap, BTreeSet};
use tracing::error;

pub trait ElectoralApi<Instance: 'static>
where
	state_chain_runtime::Runtime: pallet_cf_elections::Config<Instance>,
{
	/// Returns information about all the current elections from the perspective of this validator.
	fn electoral_data(
		&self,
		block: BlockInfo,
	) -> impl std::future::Future<
		Output = Option<ElectoralDataFor<state_chain_runtime::Runtime, Instance>>,
	> + Send
	       + 'static;

	/// Returns the subset of proposed_votes that need to be submitted.
	fn filter_votes(
		&self,
		proposed_votes: BTreeMap<
			ElectionIdentifierOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<Instance>>::ElectoralSystemRunner>,
			VoteOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<Instance>>::ElectoralSystemRunner>,
		>,
	) -> impl std::future::Future<Output = BTreeSet<ElectionIdentifierOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<Instance>>::ElectoralSystemRunner>>> + Send + 'static;
}

impl<
		RawRpcClient: RawRpcApi + Send + Sync + 'static,
		SignedExtrinsicClient: SignedExtrinsicApi + Send + Sync + 'static,
	> ElectoralApi<SolanaInstance>
	for StateChainClient<SignedExtrinsicClient, BaseRpcClient<RawRpcClient>>
{
	fn electoral_data(
		&self,
		block: BlockInfo,
	) -> impl std::future::Future<
		Output = Option<ElectoralDataFor<state_chain_runtime::Runtime, SolanaInstance>>,
	> + Send
	       + 'static {
		let base_rpc_client = self.base_rpc_client.clone();
		let account_id = self.signed_extrinsic_client.account_id();
		async move {
			base_rpc_client
				.raw_rpc_client
				.cf_solana_electoral_data(account_id, Some(block.hash))
				.await
				.map_err(anyhow::Error::from)
				.and_then(|electoral_data| <Option<ElectoralDataFor<state_chain_runtime::Runtime, SolanaInstance>> as Decode>::decode(&mut &electoral_data[..]).map_err(Into::into))
				.inspect_err(|error| {
					error!("Failure in electoral_data rpc: '{}'", error);
				})
				.ok()
				.flatten()
		}
	}

	fn filter_votes(
		&self,
		proposed_votes: BTreeMap<
			ElectionIdentifierOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<SolanaInstance>>::ElectoralSystemRunner>,
			VoteOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<SolanaInstance>>::ElectoralSystemRunner>,
		>,
	) -> impl std::future::Future<Output = BTreeSet<ElectionIdentifierOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<SolanaInstance>>::ElectoralSystemRunner>>> + Send + 'static{
		let base_rpc_client = self.base_rpc_client.clone();
		let account_id = self.signed_extrinsic_client.account_id();
		async move {
			base_rpc_client
				.raw_rpc_client
				.cf_solana_filter_votes(account_id, proposed_votes.encode(), None)
				.await
				.map_err(anyhow::Error::from)
				.and_then(|electoral_data| {
					<BTreeSet<
						ElectionIdentifierOf<
							<state_chain_runtime::Runtime as pallet_cf_elections::Config<
								SolanaInstance,
							>>::ElectoralSystemRunner,
						>,
					> as Decode>::decode(&mut &electoral_data[..])
					.map_err(Into::into)
				})
				.inspect_err(|error| {
					error!("Failure in filter_votes rpc: '{}'", error);
				})
				.unwrap_or_default()
		}
	}
}
