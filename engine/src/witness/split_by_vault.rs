use cf_primitives::EpochIndex;
use futures_util::{stream, StreamExt};

use super::{
	chain_source::{box_chain_stream, BoxChainStream, ChainSourceWithClient},
	common::{BoxCurrentAndFuture, CurrentAndFuture, ExternalChainSource, RuntimeHasInstance},
	epoch_source::Vault,
};

#[async_trait::async_trait]
pub trait ChainSplitByVault<'a>
where
	state_chain_runtime::Runtime:
		pallet_cf_vaults::Config<<Self::UnderlyingChainSource as ExternalChainSource>::Instance>,
{
	type UnderlyingChainSource: ExternalChainSource;

	async fn stream(self) -> BoxCurrentAndFuture<'a, Item<'a, Self::UnderlyingChainSource>>;
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
impl<'a, UnderlyingChainSource: ExternalChainSource> SplitByVault<'a, UnderlyingChainSource>
where
	state_chain_runtime::Runtime: RuntimeHasInstance<UnderlyingChainSource::Instance>,
{
	async fn into_item(
		underlying_chain_source: &'a UnderlyingChainSource,
		mut vault: Vault<UnderlyingChainSource::Instance>,
	) -> Item<'a, UnderlyingChainSource> {
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
	}
}
#[async_trait::async_trait]
impl<'a, UnderlyingChainSource: ExternalChainSource> ChainSplitByVault<'a>
	for SplitByVault<'a, UnderlyingChainSource>
where
	state_chain_runtime::Runtime: RuntimeHasInstance<UnderlyingChainSource::Instance>,
{
	type UnderlyingChainSource = UnderlyingChainSource;

	async fn stream(self) -> BoxCurrentAndFuture<'a, Item<'a, Self::UnderlyingChainSource>> {
		CurrentAndFuture {
			current: Box::new(
				stream::iter(self.vaults.current)
					.then(|vault| Self::into_item(self.underlying_chain_source, vault))
					.collect::<Vec<_>>()
					.await
					.into_iter(),
			),
			future: Box::pin(
				self.vaults
					.future
					.then(|vault| Self::into_item(self.underlying_chain_source, vault)),
			),
		}
	}
}
