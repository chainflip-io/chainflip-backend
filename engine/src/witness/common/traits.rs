use std::fmt::Debug;

use anyhow::Result;
use pallet_cf_elections::electoral_systems::block_height_witnesser::{
	primitives::Header, ChainBlockHashOf, ChainTypes,
};

#[async_trait::async_trait]
pub trait WitnessClient<Chain: ChainTypes>: Sync + Send {
	/// Information that's required when querying for a block.
	/// This has to contain at least the hash, but could contain more,
	/// such as bloom filters for Ethereum.
	type BlockQuery: Sync + Send + Clone + Debug;

	// ---- methods for BHW ---

	async fn best_block_number(&self) -> Result<Chain::ChainBlockNumber>;
	async fn best_block_header(&self) -> Result<Header<Chain>>;
	async fn block_header_by_height(
		&self,
		height: Chain::ChainBlockNumber,
	) -> Result<Header<Chain>>;

	// ---- methods for BW ---

	async fn block_query_from_hash_and_height(
		&self,
		hash: Chain::ChainBlockHash,
		height: Chain::ChainBlockNumber,
	) -> Result<Self::BlockQuery>;

	async fn block_query_from_height(
		&self,
		height: Chain::ChainBlockNumber,
	) -> Result<Self::BlockQuery>;

	async fn block_query_and_hash_from_height(
		&self,
		height: Chain::ChainBlockNumber,
	) -> Result<(Self::BlockQuery, ChainBlockHashOf<Chain>)>;
}

#[async_trait::async_trait]
pub trait WitnessClientForBlockData<Chain: ChainTypes, BlockData>: WitnessClient<Chain> {
	type Config: Sync + Send + Clone = ();
	type ElectionProperties = ();

	async fn block_data_from_query(
		&self,
		config: &Self::Config,
		election_properties: &Self::ElectionProperties,
		query: &Self::BlockQuery,
	) -> Result<BlockData>;
}
