use cf_primitives::EpochIndex;
use futures_util::StreamExt;

use super::{
	chain_source::{box_chain_stream, BoxChainStream, ChainSourceWithClient},
	common::{BoxCurrentAndFuture, ExternalChainSource, RuntimeHasInstance},
	epoch_source::Vault,
};

use utilities::assert_stream_send;

#[async_trait::async_trait]
pub trait ChainSplitByVault<'a>: Sized + Send
where
	state_chain_runtime::Runtime:
		pallet_cf_vaults::Config<<Self::UnderlyingChainSource as ExternalChainSource>::Instance>,
{
	type UnderlyingChainSource: ExternalChainSource;

	async fn stream(self) -> BoxCurrentAndFuture<'a, Item<'a, Self::UnderlyingChainSource>>;

	async fn run(self) {
		let stream = assert_stream_send(
			self.stream()
				.await
				.into_stream()
				.flat_map_unordered(None, |(_, _, chain_stream, _)| chain_stream),
		);
		stream.for_each(|_| futures::future::ready(())).await;
	}
}

type Item<'a, UnderlyingChainSource> = (
	EpochIndex,
	pallet_cf_vaults::Vault<
		<state_chain_runtime::Runtime as pallet_cf_vaults::Config<
			<UnderlyingChainSource as ExternalChainSource>::Instance,
		>>::Chain,
	>,
	BoxChainStream<
		'a,
		<UnderlyingChainSource as ChainSourceWithClient>::Index,
		<UnderlyingChainSource as ChainSourceWithClient>::Hash,
		<UnderlyingChainSource as ChainSourceWithClient>::Data,
	>,
	<UnderlyingChainSource as ChainSourceWithClient>::Client,
);

pub struct SplitByVault<'a, UnderlyingChainSource: ExternalChainSource>
where
	state_chain_runtime::Runtime: RuntimeHasInstance<UnderlyingChainSource::Instance>,
{
	underlying_chain_source: &'a UnderlyingChainSource,
	vaults: BoxCurrentAndFuture<'static, Vault<UnderlyingChainSource::Instance>>,
}
#[async_trait::async_trait]
impl<'a, UnderlyingChainSource: ExternalChainSource> ChainSplitByVault<'a>
	for SplitByVault<'a, UnderlyingChainSource>
where
	state_chain_runtime::Runtime: RuntimeHasInstance<UnderlyingChainSource::Instance>,
{
	type UnderlyingChainSource = UnderlyingChainSource;

	async fn stream(self) -> BoxCurrentAndFuture<'a, Item<'a, Self::UnderlyingChainSource>> {
		let underlying_chain_source = self.underlying_chain_source;
		self.vaults
			.then(move |mut vault| async move {
				let (stream, client) = underlying_chain_source.stream_and_client().await;

				(
					vault.epoch,
					vault.active_state.clone(),
					box_chain_stream(stream.take_until(vault.expired_signal.wait()).filter(
						move |header| {
							futures::future::ready(
								header.index >= vault.active_state.active_from_block &&
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
