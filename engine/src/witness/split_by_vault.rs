use cf_chains::Chain;
use futures_util::StreamExt;

use super::{
	chain_source::{box_chain_stream, BoxChainStream, ChainSource},
	common::{BoxActiveAndFuture, ExternalChainSource, RuntimeHasChain},
	epoch_source::{Epoch, Vault},
};

use utilities::assert_stream_send;

#[async_trait::async_trait]
pub trait ChainSplitByVault<'a>: Sized + Send
where
	state_chain_runtime::Runtime:
		RuntimeHasChain<<Self::UnderlyingChainSource as ExternalChainSource>::Chain>,
{
	type UnderlyingChainSource: ExternalChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::UnderlyingChainSource>>;

	async fn run(self) {
		let stream = assert_stream_send(
			self.stream()
				.await
				.into_stream()
				.flat_map_unordered(None, |(_, chain_stream, _)| chain_stream),
		);
		stream.for_each(|_| futures::future::ready(())).await;
	}
}

type Item<'a, UnderlyingChainSource> = (
	Epoch<
		pallet_cf_vaults::Vault<<UnderlyingChainSource as ExternalChainSource>::Chain>,
		<<UnderlyingChainSource as ExternalChainSource>::Chain as Chain>::ChainBlockNumber,
	>,
	BoxChainStream<
		'a,
		<UnderlyingChainSource as ChainSource>::Index,
		<UnderlyingChainSource as ChainSource>::Hash,
		<UnderlyingChainSource as ChainSource>::Data,
	>,
	<UnderlyingChainSource as ChainSource>::Client,
);

pub struct SplitByVault<'a, UnderlyingChainSource: ExternalChainSource>
where
	state_chain_runtime::Runtime: RuntimeHasChain<UnderlyingChainSource::Chain>,
{
	underlying_chain_source: &'a UnderlyingChainSource,
	vaults: BoxActiveAndFuture<'static, Vault<UnderlyingChainSource::Chain>>,
}
#[async_trait::async_trait]
impl<'a, UnderlyingChainSource: ExternalChainSource> ChainSplitByVault<'a>
	for SplitByVault<'a, UnderlyingChainSource>
where
	state_chain_runtime::Runtime: RuntimeHasChain<UnderlyingChainSource::Chain>,
{
	type UnderlyingChainSource = UnderlyingChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::UnderlyingChainSource>> {
		let underlying_chain_source = self.underlying_chain_source;
		self.vaults
			.then(move |mut vault| async move {
				let (stream, client) = underlying_chain_source.stream_and_client().await;

				(
					vault.clone(),
					box_chain_stream(stream.take_until(vault.expired_signal.wait()).filter(
						move |header| {
							futures::future::ready(
								header.index >= vault.info.active_from_block &&
									vault
										.historic_signal
										.get()
										.map_or(true, |end_index| header.index < *end_index),
							)
						},
					)),
					client,
				)
			})
			.await
			.into_box()
	}
}
