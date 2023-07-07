use futures_core::Future;
use utilities::task_scope::Scope;

use crate::witness::{
	chunked_chain_source::{chunked_by_time::ChunkByTime, chunked_by_vault::ChunkByVault},
	common::{ExternalChainSource, RuntimeHasChain},
	epoch_source::{EpochSource, VaultSource},
};

use super::{
	aliases, lag_safety::LagSafety, map::Map, shared::SharedSource,
	strictly_monotonic::StrictlyMonotonic, ChainSource, Header,
};

#[async_trait::async_trait]
pub trait ChainSourceExt: ChainSource {
	fn map<MappedTo, FutMappedTo, MapFn>(self, map_fn: MapFn) -> Map<Self, MapFn>
	where
		Self: Sized,
		MappedTo: aliases::Data,
		FutMappedTo: Future<Output = MappedTo> + Send + Sync,
		MapFn: Fn(Header<Self::Index, Self::Hash, Self::Data>) -> FutMappedTo + Send + Sync + Clone,
	{
		Map::new(self, map_fn)
	}

	fn lag_safety(self, margin: usize) -> LagSafety<Self>
	where
		Self: Sized,
	{
		LagSafety::new(self, margin)
	}

	fn shared<'env>(self, scope: &Scope<'env, anyhow::Error>) -> SharedSource<Self>
	where
		Self: 'env + Sized,
		Self::Client: Clone,
		Self::Data: Clone,
	{
		SharedSource::new(self, scope)
	}

	fn strictly_monotonic(self) -> StrictlyMonotonic<Self>
	where
		Self: Sized,
	{
		StrictlyMonotonic::new(self)
	}

	async fn chunk_by_time<'a, 'b, 'env, StateChainClient: Send + Sync>(
		&'a self,
		epochs: EpochSource<'b, 'env, StateChainClient, (), ()>,
	) -> ChunkByTime<'a, Self>
	where
		Self: Sized,
	{
		ChunkByTime::new(self, epochs.into_stream().await.into_box())
	}

	async fn chunk_by_vault<'a, 'b, 'env, StateChainClient: Send + Sync>(
		&'a self,
		vaults: VaultSource<'b, 'env, StateChainClient, Self::Chain>,
	) -> ChunkByVault<'a, Self>
	where
		Self: Sized + ExternalChainSource,
		state_chain_runtime::Runtime: RuntimeHasChain<Self::Chain>,
	{
		ChunkByVault::new(self, vaults.into_stream().await.into_box())
	}
}
