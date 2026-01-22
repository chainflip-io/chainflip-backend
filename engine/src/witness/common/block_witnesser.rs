use pallet_cf_elections::electoral_systems::{block_height_witnesser::{ChainTypes, primitives::Header}, block_witnesser::state_machine::{BWProcessorTypes, EngineElectionType}, state_machine::state_machine_es::{StatemachineElectoralSystem, StatemachineElectoralSystemTypes}};
use crate::{elections::voter_api::VoterApi, witness::common::block_height_witnesser::HeaderClient};

use anyhow::Result;


#[async_trait::async_trait]
pub trait BlockClient<Chain: ChainTypes, Block> {
    async fn get_block(header: Header<Chain>) -> Block;
}


pub async fn query_block_header_for_bw<Chain: ChainTypes, Client: HeaderClient<Chain>>(
    client: &Client,
    block_height: Chain::ChainBlockNumber,
    election_type: EngineElectionType<Chain>
) -> Result<Header<Chain>> {
    match election_type {
        EngineElectionType::ByHash(hash) => {
            let header = client.block_header_by_hash(&hash).await?;

            if header.hash != hash {
                return Err(anyhow::anyhow!(
                    "Block hash from RPC ({:?}) doesn't match election block hash: {:?}",
                    header.hash,
                    hash
                ));
            }

            Ok(header)
        },
        EngineElectionType::BlockHeight { submit_hash } => {
            let header = client.block_header_by_height(block_height).await?;
            if header.block_height != block_height {
                return Err(anyhow::anyhow!(
                    "Block number from RPC ({:?}) doesn't match election block height: {:?}",
                    header.block_height,
                    block_height
                ));
            }
            Ok(header)
        },
    }
}

impl<T: BWProcessorTypes + StatemachineElectoralSystemTypes, Client: HeaderClient<T::Chain>> VoterApi<StatemachineElectoralSystem<T>> for Client {
    async fn vote(
            &self,
            settings: <StatemachineElectoralSystem<T> as pallet_cf_elections::ElectoralSystemTypes>::ElectoralSettings,
            properties: <StatemachineElectoralSystem<T> as pallet_cf_elections::ElectoralSystemTypes>::ElectionProperties,
        ) -> Result<Option<pallet_cf_elections::VoteOf<StatemachineElectoralSystem<T>>>> {
        todo!()
    }
}




