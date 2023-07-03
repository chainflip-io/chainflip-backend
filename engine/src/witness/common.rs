use cf_chains::{Chain, ChainAbi};
use futures_core::{stream::BoxStream, Future, Stream};
use futures_util::{stream, StreamExt};
use state_chain_runtime::PalletInstanceAlias;

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

	pub async fn filter<Fut: Future<Output = bool>, F: Fn(&It::Item) -> Fut>(
		self,
		f: F,
	) -> ActiveAndFuture<std::vec::IntoIter<It::Item>, stream::Filter<St, Fut, F>> {
		ActiveAndFuture {
			active: stream::iter(self.active).filter(&f).collect::<Vec<_>>().await.into_iter(),
			future: self.future.filter(f),
		}
	}

	pub async fn then<Fut: Future, F: Fn(It::Item) -> Fut>(
		self,
		f: F,
	) -> ActiveAndFuture<std::vec::IntoIter<Fut::Output>, stream::Then<St, Fut, F>> {
		ActiveAndFuture {
			active: stream::iter(self.active).then(&f).collect::<Vec<_>>().await.into_iter(),
			future: self.future.then(f),
		}
	}
}

pub type BoxActiveAndFuture<'a, T> =
	ActiveAndFuture<Box<dyn Iterator<Item = T> + Send + 'a>, BoxStream<'a, T>>;

pub trait RuntimeHasChain<TChain: ExternalChain>:
	pallet_cf_vaults::Config<<TChain as PalletInstanceAlias>::Instance, Chain = TChain>
{
}
impl<TChain: ExternalChain> RuntimeHasChain<TChain> for state_chain_runtime::Runtime where
	Self: pallet_cf_vaults::Config<<TChain as PalletInstanceAlias>::Instance, Chain = TChain>
{
}

pub trait ExternalChain: ChainAbi + PalletInstanceAlias {}
impl<T: ChainAbi + PalletInstanceAlias> ExternalChain for T {}

pub trait ExternalChainSource:
	ChainSourceWithClient<Index = <Self::Chain as Chain>::ChainBlockNumber>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Self::Chain>,
{
	type Chain: ExternalChain;
}
