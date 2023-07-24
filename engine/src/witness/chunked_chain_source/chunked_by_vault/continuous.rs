use crate::witness::chunked_chain_source::chunked_by_vault::builder::ChunkedByVaultBuilder;
use cf_primitives::EpochIndex;
use core::iter::Step;
use futures_util::stream;
use std::sync::Arc;
use utilities::{future_map::FutureMap, loop_select, rle_bitmap::RleBitmap};

use super::ChunkedByVault;
use crate::{
	db::PersistentKeyDB,
	witness::chain_source::{ChainClient, ChainStream},
};
use futures::{FutureExt, StreamExt};
use serde::{de::DeserializeOwned, Serialize};
use utilities::UnendingStream;

pub trait Storage<Index: Ord>: Send + Sync {
	fn load(&self, epoch: EpochIndex) -> Result<Option<RleBitmap<Index>>, anyhow::Error>;
	fn store(&self, epoch: EpochIndex, map: &RleBitmap<Index>) -> Result<(), anyhow::Error>;
}

impl<Index: Ord + DeserializeOwned + Serialize> Storage<Index> for (String, Arc<PersistentKeyDB>) {
	fn load(&self, epoch: EpochIndex) -> Result<Option<RleBitmap<Index>>, anyhow::Error> {
		self.1.load_processed_blocks(&self.0, epoch)
	}
	fn store(&self, epoch: EpochIndex, map: &RleBitmap<Index>) -> Result<(), anyhow::Error> {
		self.1.update_processed_blocks(&self.0, epoch, map)
	}
}

pub struct Continuous<Inner, Store> {
	inner: Inner,
	store: Store,
}
impl<Inner: ChunkedByVault, Store: Storage<Inner::Index>> Continuous<Inner, Store> {
	pub fn new(inner: Inner, store: Store) -> Self {
		Self { inner, store }
	}
}
#[async_trait::async_trait]
impl<Inner: ChunkedByVault, Store: Storage<Inner::Index>> ChunkedByVault
	for Continuous<Inner, Store>
where
	Inner::Client: Clone,
{
	type Info = Inner::Info;
	type HistoricInfo = Inner::HistoricInfo;

	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = Inner::Data;

	type Client = Inner::Client;

	type Chain = Inner::Chain;

	type Parameters = Inner::Parameters;

	async fn stream(
		&self,
		parameters: Self::Parameters,
	) -> crate::witness::common::BoxActiveAndFuture<'_, super::Item<'_, Self>> {
		self.inner
			.stream(parameters)
			.await
			.then(move |(epoch, chain_stream, chain_client)| async move {
				const MAXIMUM_CONCURRENT_INPROGRESS: usize = 32;

				let processed_indices = self.store.load(epoch.index).map_or(RleBitmap::new(false), |option_processed_indices| {
					option_processed_indices.unwrap_or(RleBitmap::new(false))
				});

				let inprogress_indices = FutureMap::default();

				let unprocessed_indices = {
					let mut processed_inverse = processed_indices.clone();
					processed_inverse.invert();
					processed_inverse.set_range(..epoch.info.0.active_from_block, false);
					let highest_processed = processed_indices.iter(true).last().map_or(epoch.info.0.active_from_block, |highest_processed| std::cmp::max(highest_processed, epoch.info.0.active_from_block));
					processed_inverse.set_range(highest_processed.., false);
					processed_inverse
				};

				(
					epoch.clone(),
					stream::unfold(
						(chain_stream.fuse(), chain_client.clone(), epoch, unprocessed_indices, inprogress_indices, processed_indices),
						move |(mut chain_stream, chain_client, mut epoch, mut unprocessed_indices, mut inprogress_indices, mut processed_indices)| async move {
							loop_select!(
								let header = chain_stream.next_or_pending() => {
									let highest_processed = processed_indices.iter(true).last().map_or(epoch.info.0.active_from_block, |highest_processed| std::cmp::max(highest_processed, epoch.info.0.active_from_block));
									if highest_processed < header.index {
										for unprocessed_index in Step::forward(highest_processed, 1)..header.index {
											if inprogress_indices.len() < MAXIMUM_CONCURRENT_INPROGRESS {
												inprogress_indices.insert(unprocessed_index, {
													let chain_client = chain_client.clone();
													#[allow(clippy::redundant_async_block)]
													async move {
														chain_client.header_at_index(unprocessed_index).await
													}.boxed()
												});
											} else {
												unprocessed_indices.set(unprocessed_index, true);
											}
										}
									}

									unprocessed_indices.set(header.index, false);
									inprogress_indices.remove(header.index);
									processed_indices.set(header.index, true);
									let _result = self.store.store(epoch.index, &processed_indices);

									break Some((header, (chain_stream, chain_client, epoch, unprocessed_indices, inprogress_indices, processed_indices)))
								},
								if epoch.historic_signal.get().is_some() && processed_indices.is_superset(&{
									let mut bitmap = RleBitmap::new(true);
									bitmap.set_range(..epoch.info.0.active_from_block, false);
									bitmap.set_range((*epoch.historic_signal.get().unwrap()).0.., false);
									bitmap
								}) => break None,
								let (_, header) = inprogress_indices.next_or_pending() => {
									processed_indices.set(header.index, true);
									let _result = self.store.store(epoch.index, &processed_indices);

									let next_unprocessed_indice = unprocessed_indices.iter(true).next();
									if let Some(unprocessed_index) = next_unprocessed_indice {
										unprocessed_indices.set(unprocessed_index, false);
										inprogress_indices.insert(unprocessed_index, {
											let chain_client = chain_client.clone();
											#[allow(clippy::redundant_async_block)]
											async move {
												chain_client.header_at_index(unprocessed_index).await
											}.boxed()
										});
									}
									break Some((header, (chain_stream, chain_client, epoch, unprocessed_indices, inprogress_indices, processed_indices)))
								},
							)
						},
					)
					.into_box(),
					chain_client,
				)
			})
			.await
			.into_box()
	}
}

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub fn continuous(
		self,
		name: String,
		db: Arc<PersistentKeyDB>,
	) -> ChunkedByVaultBuilder<Continuous<Inner, (String, Arc<PersistentKeyDB>)>>
	where
		Inner::Client: Clone,
	{
		ChunkedByVaultBuilder::new(Continuous::new(self.source, (name, db)), self.parameters)
	}
}
