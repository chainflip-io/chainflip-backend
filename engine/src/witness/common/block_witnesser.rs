use crate::{elections::voter_api::VoterApi, witness::common::traits::WitnessClientForBlockData};
use pallet_cf_elections::{
	electoral_systems::{
		block_height_witnesser::ChainTypes,
		block_witnesser::{
			instance::{BlockWitnesserInstance, GenericBlockWitnesser},
			state_machine::EngineElectionType,
		},
		state_machine::state_machine_es::StatemachineElectoralSystem,
	},
	VoteOf,
};

use anyhow::Result;

#[derive(Clone)]
pub struct GenericBwVoter<I: BlockWitnesserInstance<Chain: ChainTypes>, Client: BlockClientFor<I>> {
	client: Client,
	_phantom: std::marker::PhantomData<I>,
}

impl<I: BlockWitnesserInstance<Chain: ChainTypes>, Client: BlockClientFor<I>>
	GenericBwVoter<I, Client>
{
	pub fn new(client: Client) -> Self {
		Self { client, _phantom: Default::default() }
	}
}

pub trait BlockClientFor<I: BlockWitnesserInstance<Chain: ChainTypes>> =
	WitnessClientForBlockData<I::Chain, I::ElectionProperties, Vec<I::BlockEntry>>;

#[async_trait::async_trait]
impl<I: BlockWitnesserInstance<Chain: ChainTypes>, Client: BlockClientFor<I>>
	VoterApi<StatemachineElectoralSystem<GenericBlockWitnesser<I>>> for GenericBwVoter<I, Client>
{
	async fn vote(
		&self,
		_settings: <StatemachineElectoralSystem<GenericBlockWitnesser<I>> as pallet_cf_elections::ElectoralSystemTypes>::ElectoralSettings,
		properties: <StatemachineElectoralSystem<GenericBlockWitnesser<I>> as pallet_cf_elections::ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<StatemachineElectoralSystem<GenericBlockWitnesser<I>>>>, anyhow::Error>
	{
		match properties.election_type {
			EngineElectionType::ByHash(ref hash) => {
				let query = self
					.client
					.block_query_from_hash_and_height(hash.clone(), properties.block_height)
					.await?;
				let data =
					self.client.block_data_from_query(&properties.properties, &query).await?;
				Ok(Some((data, None)))
			},
			EngineElectionType::BlockHeight { submit_hash: false } => {
				let query = self.client.block_query_from_height(properties.block_height).await?;
				let data =
					self.client.block_data_from_query(&properties.properties, &query).await?;
				Ok(Some((data, None)))
			},
			EngineElectionType::BlockHeight { submit_hash: true } => {
				// optimistic election: check whether block exists with the given height
				let best_block_number = self.client.best_block_number().await?;
				if best_block_number < properties.block_height {
					return Ok(None)
				}
				// query actual data
				let (query, hash) =
					self.client.block_query_and_hash_from_height(properties.block_height).await?;
				let data =
					self.client.block_data_from_query(&properties.properties, &query).await?;
				Ok(Some((data, Some(hash))))
			},
		}
	}
}
