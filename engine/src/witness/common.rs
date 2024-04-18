pub mod chain_source;
pub mod chunked_chain_source;
pub mod epoch_source;

use cf_chains::{
	instances::{ChainInstanceAlias, ChainInstanceFor, CryptoInstanceFor},
	Chain,
};
use futures_core::{stream::BoxStream, Future, Stream};
use futures_util::{stream, StreamExt};

use chain_source::ChainSource;

#[derive(Clone)]
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
	pallet_cf_vaults::Config<ChainInstanceFor<TChain>, Chain = TChain>
	+ pallet_cf_threshold_signature::Config<
		CryptoInstanceFor<TChain>,
		TargetChainCrypto = TChain::ChainCrypto,
	> + pallet_cf_chain_tracking::Config<ChainInstanceFor<TChain>, TargetChain = TChain>
	+ pallet_cf_ingress_egress::Config<ChainInstanceFor<TChain>, TargetChain = TChain>
	+ pallet_cf_broadcast::Config<ChainInstanceFor<TChain>, TargetChain = TChain>
{
}
impl<TChain: ExternalChain> RuntimeHasChain<TChain> for state_chain_runtime::Runtime where
	Self: pallet_cf_vaults::Config<ChainInstanceFor<TChain>, Chain = TChain>
		+ pallet_cf_threshold_signature::Config<
			CryptoInstanceFor<TChain>,
			TargetChainCrypto = TChain::ChainCrypto,
		> + pallet_cf_chain_tracking::Config<ChainInstanceFor<TChain>, TargetChain = TChain>
		+ pallet_cf_ingress_egress::Config<ChainInstanceFor<TChain>, TargetChain = TChain>
		+ pallet_cf_broadcast::Config<ChainInstanceFor<TChain>, TargetChain = TChain>
{
}

pub trait RuntimeCallHasChain<Runtime: RuntimeHasChain<TChain>, TChain: ExternalChain>:
	std::convert::From<pallet_cf_vaults::Call<Runtime, ChainInstanceFor<TChain>>>
	+ std::convert::From<pallet_cf_chain_tracking::Call<Runtime, ChainInstanceFor<TChain>>>
	+ std::convert::From<pallet_cf_broadcast::Call<Runtime, ChainInstanceFor<TChain>>>
	+ std::convert::From<pallet_cf_ingress_egress::Call<Runtime, ChainInstanceFor<TChain>>>
{
}
impl<Runtime: RuntimeHasChain<TChain>, TChain: ExternalChain> RuntimeCallHasChain<Runtime, TChain>
	for state_chain_runtime::RuntimeCall
where
	Self: std::convert::From<pallet_cf_vaults::Call<Runtime, ChainInstanceFor<TChain>>>
		+ std::convert::From<pallet_cf_chain_tracking::Call<Runtime, ChainInstanceFor<TChain>>>
		+ std::convert::From<pallet_cf_broadcast::Call<Runtime, ChainInstanceFor<TChain>>>
		+ std::convert::From<pallet_cf_ingress_egress::Call<Runtime, ChainInstanceFor<TChain>>>,
{
}

pub trait ExternalChain: Chain + ChainInstanceAlias {}
impl<T: Chain + ChainInstanceAlias> ExternalChain for T {}

pub trait ExternalChainSource:
	ChainSource<Index = <Self::Chain as Chain>::ChainBlockNumber>
{
	type Chain: ExternalChain;
}
