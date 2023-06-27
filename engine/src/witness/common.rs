use cf_chains::Chain;
use futures_core::{stream::BoxStream, Future, Stream};
use futures_util::{stream, StreamExt};

use super::chain_source::ChainSourceWithClient;

pub const STATE_CHAIN_CONNECTION: &str = "State Chain client connection failed"; // TODO Replace with infallible SCC requests

pub struct ActiveAndFuture<It: Iterator, St: Stream<Item = It::Item>> {
	pub active: It,
	pub future: St,
}
impl<It: Iterator, St: Stream<Item = It::Item>> ActiveAndFuture<It, St> {
	pub fn into_stream(self) -> impl Stream<Item = It::Item> {
		stream::iter(self.active).chain(self.future)
	}

	pub fn into_box<'a>(self) -> BoxActiveAndFuture<'a, It::Item>
	where
		It::Item: 'a,
		It: Send + 'a,
		St: Send + 'a,
	{
		ActiveAndFuture { active: Box::new(self.active), future: Box::pin(self.future) }
	}

	pub async fn then<Fut: Future, F: Fn(It::Item) -> Fut>(
		self,
		f: F,
	) -> ActiveAndFuture<impl Iterator<Item = Fut::Output>, impl Stream<Item = Fut::Output>> {
		ActiveAndFuture {
			active: stream::iter(self.active).then(&f).collect::<Vec<_>>().await.into_iter(),
			future: self.future.then(f),
		}
	}
}

pub type BoxActiveAndFuture<'a, T> =
	ActiveAndFuture<Box<dyn Iterator<Item = T> + Send + 'a>, BoxStream<'a, T>>;

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
