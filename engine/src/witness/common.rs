use cf_chains::Chain;
use futures_core::{stream::BoxStream, Stream};
use futures_util::{stream, StreamExt};

use super::chain_source::ChainSourceWithClient;

pub const STATE_CHAIN_CONNECTION: &str = "State Chain client connection failed"; // TODO Replace with infallible SCC requests

pub struct CurrentAndFuture<It: Iterator, St: Stream<Item = It::Item>> {
	pub current: It,
	pub future: St,
}
impl<It: Iterator, St: Stream<Item = It::Item>> CurrentAndFuture<It, St> {
	pub fn into_stream(self) -> impl Stream<Item = It::Item> {
		stream::iter(self.current).chain(self.future)
	}
}

pub type BoxCurrentAndFuture<'a, T> =
	CurrentAndFuture<Box<dyn Iterator<Item = T> + Send + 'a>, BoxStream<'a, T>>;

pub trait RuntimeHasInstance<Instance: 'static>: pallet_cf_vaults::Config<Instance> {}
impl<Instance: 'static> RuntimeHasInstance<Instance> for state_chain_runtime::Runtime where
	Self: pallet_cf_vaults::Config<Instance>
{
}

pub trait ExternalChainSource: ChainSourceWithClient<Index = <<state_chain_runtime::Runtime as pallet_cf_vaults::Config<Self::Instance>>::Chain as Chain>::ChainBlockNumber>
where
	state_chain_runtime::Runtime: RuntimeHasInstance<Self::Instance>,
{
	type Instance: 'static;
}
