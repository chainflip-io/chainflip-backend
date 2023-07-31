use futures_core::Future;
use utilities::task_scope::Scope;

use crate::witness::common::{
	chunked_chain_source::{
		chunked_by_time::{builder::ChunkedByTimeBuilder, ChunkByTime},
		chunked_by_vault::{builder::ChunkedByVaultBuilder, ChunkByVault},
	},
	epoch_source::{EpochSource, VaultSource},
	ExternalChainSource, RuntimeHasChain,
};

use super::{
	aliases, and_then::AndThen, lag_safety::LagSafety, shared::SharedSource,
	strictly_monotonic::StrictlyMonotonic, then::Then, ChainSource, Header,
};

#[async_trait::async_trait]
pub trait ChainSourceExt: ChainSource {
	fn then<Output, Fut, F>(self, f: F) -> Then<Self, F>
	where
		Self: Sized,
		Output: aliases::Data,
		Fut: Future<Output = Output> + Send,
		F: Fn(Header<Self::Index, Self::Hash, Self::Data>) -> Fut + Send + Sync + Clone,
	{
		Then::new(self, f)
	}

	fn and_then<Input, Output, Error, Fut, F>(self, f: F) -> AndThen<Self, F>
	where
		Self: Sized + ChainSource<Data = Result<Input, Error>>,
		Input: aliases::Data,
		Output: aliases::Data,
		Error: aliases::Data,
		Fut: Future<Output = Result<Output, Error>> + Send,
		F: Fn(Header<Self::Index, Self::Hash, Input>) -> Fut + Send + Sync + Clone,
	{
		AndThen::new(self, f)
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

	fn chunk_by_time<Epochs: Into<EpochSource<(), ()>>>(
		self,
		epochs: Epochs,
	) -> ChunkedByTimeBuilder<ChunkByTime<Self>>
	where
		Self: ExternalChainSource + Sized,
	{
		ChunkedByTimeBuilder::new(ChunkByTime::new(self), epochs.into())
	}

	fn chunk_by_vault<
		ExtraInfo,
		ExtraHistoricInfo,
		Vaults: Into<VaultSource<Self::Chain, ExtraInfo, ExtraHistoricInfo>>,
	>(
		self,
		vaults: Vaults,
	) -> ChunkedByVaultBuilder<ChunkByVault<Self, ExtraInfo, ExtraHistoricInfo>>
	where
		Self: ExternalChainSource + Sized,
		state_chain_runtime::Runtime: RuntimeHasChain<Self::Chain>,
		ExtraInfo: Clone + Send + Sync + 'static,
		ExtraHistoricInfo: Clone + Send + Sync + 'static,
	{
		ChunkedByVaultBuilder::new(ChunkByVault::new(self), vaults.into())
	}
}
impl<T: ChainSource> ChainSourceExt for T {}
