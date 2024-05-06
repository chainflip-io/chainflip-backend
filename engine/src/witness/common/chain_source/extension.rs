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
	aliases, and_then::AndThen, lag_safety::LagSafety, logging::Logging, shared::SharedSource,
	strictly_monotonic::StrictlyMonotonic, then::Then, ChainSource, Header,
};

#[async_trait::async_trait]
pub trait ChainSourceExt: ChainSource {
	/// Map the data of each header with an async closure.
	fn then<Output, Fut, F>(self, f: F) -> Then<Self, F>
	where
		Self: Sized,
		Output: aliases::Data,
		Fut: Future<Output = Output> + Send,
		F: Fn(Header<Self::Index, Self::Hash, Self::Data>) -> Fut + Send + Sync + Clone,
	{
		Then::new(self, f)
	}

	/// Map the data of each header when the data is a Result::Ok with an async closure.
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

	/// Apply some safety margin to the chain source, such that the chain source will lag behind by
	/// a set margin. This is specifically for chains that don't offer deterministic finality, such
	/// as Ethereum or Bitcoin.
	fn lag_safety(
		self,
		margin: <<Self as ExternalChainSource>::Chain as cf_chains::Chain>::ChainBlockNumber,
	) -> LagSafety<Self>
	where
		Self: ExternalChainSource + Sized,
	{
		LagSafety::new(self, margin)
	}

	/// Allows sharing an underlying chain source between multiple consumers. This ensures that work
	/// done in previous chain source adapters is not duplicated by downstream consumers.
	fn shared<'env>(self, scope: &Scope<'env, anyhow::Error>) -> SharedSource<Self>
	where
		Self: 'env + Sized,
		Self::Client: Clone,
		Self::Data: Clone,
	{
		SharedSource::new(self, scope)
	}

	/// Logs when a header is produced by the underlying stream the hash and index of the header.
	/// Prefixes the log message with the given prefix.
	fn logging(self, log_prefix: &'static str) -> Logging<Self>
	where
		Self: Sized,
	{
		Logging::new(self, log_prefix)
	}

	/// Ensures the stream is always increasing with respect to the header index (normally the block
	/// number). We don't assume the root chain source is strictly increasing, since we could
	/// encounter reorgs.
	fn strictly_monotonic(self) -> StrictlyMonotonic<Self>
	where
		Self: Sized,
	{
		StrictlyMonotonic::new(self)
	}

	/// Chunk the chain source by time (in blocks). Some consumers do not care about the exact
	/// external chain block number they start and end but we only want to run it for the epoch
	/// duration (as measured approximately by the State Chain blocks we consume).
	fn chunk_by_time<'env, Epochs: Into<EpochSource<(), ()>>>(
		self,
		epochs: Epochs,
		scope: &Scope<'env, anyhow::Error>,
	) -> ChunkedByTimeBuilder<ChunkByTime<SharedSource<Self>>>
	where
		Self: ExternalChainSource + Sized + 'env,
		Self::Client: Clone,
		Self::Data: Clone,
	{
		// Note the use of the shared adapter which ensures that chunked adapter uses
		// the same underlying stream and client for each epoch:
		ChunkedByTimeBuilder::new(ChunkByTime::new(self.shared(scope)), epochs.into())
	}

	/// Chunk the chain source by vault. We specifically want to chunk the chain source from the
	/// block the epoch starts at for a particular chain. This ensures we don't miss witnesses, and
	/// allows us to only run for those epochs we are interested in.
	fn chunk_by_vault<
		'env,
		ExtraInfo,
		ExtraHistoricInfo,
		Vaults: Into<VaultSource<Self::Chain, ExtraInfo, ExtraHistoricInfo>>,
	>(
		self,
		vaults: Vaults,
		scope: &Scope<'env, anyhow::Error>,
	) -> ChunkedByVaultBuilder<ChunkByVault<SharedSource<Self>, ExtraInfo, ExtraHistoricInfo>>
	where
		Self: ExternalChainSource + Sized + 'env,
		Self::Client: Clone,
		Self::Data: Clone,
		state_chain_runtime::Runtime: RuntimeHasChain<Self::Chain>,
		ExtraInfo: Clone + Send + Sync + 'static,
		ExtraHistoricInfo: Clone + Send + Sync + 'static,
	{
		// Note the use of the shared adapter which ensures that chunked adapter uses
		// the same underlying stream and client for each epoch:
		ChunkedByVaultBuilder::new(ChunkByVault::new(self.shared(scope)), vaults.into())
	}
}
impl<T: ChainSource> ChainSourceExt for T {}
