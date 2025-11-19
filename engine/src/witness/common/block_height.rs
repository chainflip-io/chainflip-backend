use cf_chains::witness_period::SaturatingStep;
use pallet_cf_elections::{
	electoral_systems::block_height_witnesser::{
		primitives::{Header, NonemptyContinuousHeaders},
		ChainTypes, HeightWitnesserProperties,
	},
	ElectoralSystemTypes,
};
use sp_core::bounded::alloc::collections::VecDeque;

#[async_trait::async_trait]
pub trait HeaderClient<H> {
	async fn best_block_header(&self) -> anyhow::Result<H>;
	async fn block_header_by_height(&self, height: u64) -> anyhow::Result<H>;
}

pub async fn witness_headers<ES, C, RawHeader, Chain>(
	client: &C,
	properties: <ES as ElectoralSystemTypes>::ElectionProperties,
	safety_buffer: u32,
	header_conv: impl Fn(RawHeader) -> anyhow::Result<Header<Chain>>,
	tag: &'static str,
) -> anyhow::Result<Option<NonemptyContinuousHeaders<Chain>>>
where
	ES: ElectoralSystemTypes<ElectionProperties = HeightWitnesserProperties<Chain>>,
	C: HeaderClient<RawHeader>,
	Chain: ChainTypes<ChainBlockNumber = u64>,
{
	let HeightWitnesserProperties { witness_from_index } = properties;

	let best_raw = client.best_block_header().await?;
	let best_block_header = header_conv(best_raw)?;

	if best_block_header.block_height < witness_from_index {
		tracing::debug!("{tag:?}: no new blocks since best block height={:?} for witness_from={witness_from_index:?}", best_block_header.block_height);
		return Ok(None);
	}

	// The `latest_block_height == 0` is a special case for when starting up the
	// electoral system for the first time.
	let witness_from_index = if witness_from_index == 0u64 {
		tracing::debug!(
			"{tag:?}: election_property=0, best_block_height={:?}, submitting last {:?} blocks.",
			best_block_header.block_height,
			safety_buffer
		);
		best_block_header.block_height.saturating_sub(safety_buffer as u64)
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
		.map(|index| {
			let header_conv = &header_conv;
			async move {
				let raw = client.block_header_by_height(index).await?;
				header_conv(raw)
			}
		})
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
