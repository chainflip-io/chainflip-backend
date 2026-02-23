use cf_chains::witness_period::SaturatingStep;
use pallet_cf_elections::{
	electoral_systems::block_height_witnesser::{
		primitives::NonemptyContinuousHeaders, ChainTypes, HeightWitnesserProperties,
	},
	ElectoralSystemTypes,
};
use sp_core::bounded::alloc::collections::VecDeque;

use crate::witness::common::traits::WitnessClient;

pub async fn witness_headers<ES, Client, Chain>(
	client: &Client,
	properties: <ES as ElectoralSystemTypes>::ElectionProperties,
	safety_buffer: u32,
	tag: &'static str,
) -> anyhow::Result<Option<NonemptyContinuousHeaders<Chain>>>
where
	ES: ElectoralSystemTypes<ElectionProperties = HeightWitnesserProperties<Chain>>,
	Client: WitnessClient<Chain>,
	Chain: ChainTypes,
{
	let HeightWitnesserProperties { witness_from_index } = properties;

	let best_block_number = client.best_block_number().await?;
	if best_block_number < witness_from_index {
		tracing::debug!("{tag:?}: no new blocks since best block height={:?} for witness_from={witness_from_index:?}", best_block_number);
		return Ok(None)
	}
	let best_block_header = client.best_block_header().await?;

	// The `latest_block_height == 0` is a special case for when starting up the
	// electoral system for the first time.
	let witness_from_index = if witness_from_index == Default::default() {
		tracing::debug!(
			"{tag:?}: election_property=0, best_block_height={:?}, submitting last {:?} blocks.",
			best_block_header.block_height,
			safety_buffer
		);
		best_block_header.block_height.saturating_backward(safety_buffer as usize)
	} else {
		witness_from_index
	};

	// Compute the highest block height we want to fetch a header for,
	// since for performance reasons we're bounding the number of headers
	// submitted in one vote. We're submitting at most SAFETY_BUFFER headers.
	let highest_submitted_height = std::cmp::min(
		best_block_header.block_height,
		witness_from_index.saturating_forward(safety_buffer as usize + 1),
	);

	// request headers for at most SAFETY_BUFFER heights, in parallel
	let requests = (witness_from_index..highest_submitted_height)
		.map(|index| async move { client.block_header_by_height(index).await })
		.collect::<Vec<_>>();

	let mut headers: VecDeque<_> = futures::future::join_all(requests)
		.await
		.into_iter()
		.collect::<anyhow::Result<_>>()?;

	// If we submitted all headers up the highest, we also append the highest
	if highest_submitted_height == best_block_header.block_height {
		headers.push_back(best_block_header);
	}

	let headers_len = headers.len();
	NonemptyContinuousHeaders::try_new(headers)
        .inspect(|_| tracing::debug!("{tag:?}: Submitting vote for (witness_from={witness_from_index:?}) with {headers_len:?} headers"))
        .map(Some)
        .map_err(|err| anyhow::format_err!("{tag:?}: {err:?}"))
}
