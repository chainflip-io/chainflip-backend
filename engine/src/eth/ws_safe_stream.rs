use std::collections::VecDeque;

use futures::{stream, Stream};
use slog::o;
use web3::types::{BlockHeader, U64};

use futures::StreamExt;

use crate::logging::COMPONENT_KEY;

use super::EthNumberBloom;

use anyhow::Result;

pub fn safe_ws_head_stream<BlockHeaderStream>(
	header_stream: BlockHeaderStream,
	safety_margin: u64,
	logger: &slog::Logger,
) -> impl Stream<Item = EthNumberBloom>
where
	BlockHeaderStream: Stream<Item = Result<BlockHeader, web3::Error>>,
{
	struct StreamAndBlocks<BlockHeaderStream>
	where
		BlockHeaderStream: Stream<Item = Result<BlockHeader, web3::Error>>,
	{
		stream: BlockHeaderStream,
		last_block_pulled: Option<U64>,
		unsafe_block_headers: VecDeque<EthNumberBloom>,
		logger: slog::Logger,
	}
	let init_state = StreamAndBlocks {
		stream: Box::pin(header_stream),
		last_block_pulled: None,
		unsafe_block_headers: Default::default(),
		logger: logger.new(o!(COMPONENT_KEY => "ETH_WSSafeStream")),
	};

	macro_rules! break_unwrap {
		($item:expr, $name:expr, $logger:expr) => {{
			match $item {
				Some(item) => item,
				None => {
					slog::error!(
						$logger,
						"Terminating stream. Latest WS block header does not have a {}.",
						$name
					);
					break None
				},
			}
		}};
	}

	Box::pin(
		stream::unfold(init_state, move |mut state| async move {
			loop {
				if let Some(header) = state.stream.next().await {
					let current_header = match header {
						Ok(header) => header,
						Err(err) => {
							slog::error!(
								state.logger,
								"Terminating stream. Error pulling head from stream: {}",
								err
							);
							break None
						},
					};

					let current_block_number =
						break_unwrap!(current_header.number, "block number", &state.logger);
					let current_base_fee_per_gas = break_unwrap!(
						current_header.base_fee_per_gas,
						"base fee per gas",
						&state.logger
					);

					// Terminate stream if we have skipped into the future
					if let Some(last_block_pulled) = state.last_block_pulled {
						if current_block_number > last_block_pulled + 1 {
							break None
						}
					}

					state.last_block_pulled = Some(current_block_number);

					if let Some(last_unsafe_block_header) = state.unsafe_block_headers.back() {
						let last_unsafe_block_number = last_unsafe_block_header.block_number;
						assert!(current_block_number <= last_unsafe_block_number + 1);

						// if we receive two of the same block number then we still need to drop the
						// first hence + 1
						let reorg_depth =
							((last_unsafe_block_number + 1) - current_block_number).as_u64();

						if reorg_depth > safety_margin {
							break None
						} else if reorg_depth > 0 {
							(0..reorg_depth).for_each(|_| {
								state.unsafe_block_headers.pop_back();
							});
						}
					}

					state.unsafe_block_headers.push_back(EthNumberBloom {
						block_number: current_block_number,
						logs_bloom: current_header.logs_bloom,
						base_fee_per_gas: current_base_fee_per_gas,
					});

					if let Some(header) = state.unsafe_block_headers.front() {
						if header.block_number.saturating_add(U64::from(safety_margin)) <=
							current_block_number
						{
							break Some((
								state
									.unsafe_block_headers
									.pop_front()
									.expect("already checked for item above"),
								state,
							))
						} else {
							// we don't want to return None to the caller here. Instead we want to
							// keep progressing through the inner stream
							continue
						}
					}
				} else {
					// when the inner stream is consumed, we want to end the wrapping/safe stream
					break None
				}
			}
		})
		.fuse(),
	)
}

#[cfg(test)]
pub mod tests {

	use web3::types::{H160, H256, U256};

	use crate::logging::test_utils::new_test_logger;

	use super::*;

	pub fn block_header(hash: u8, block_number: u64) -> Result<BlockHeader, web3::Error> {
		let block_header = BlockHeader {
			// fields that matter
			hash: Some(H256::from([hash; 32])),
			number: Some(U64::from(block_number)),
			base_fee_per_gas: Some(U256::from(1)),

			// defaults
			logs_bloom: Default::default(),
			parent_hash: H256::default(),
			uncles_hash: H256::default(),
			author: H160::default(),
			state_root: H256::default(),
			transactions_root: H256::default(),
			receipts_root: H256::default(),
			gas_used: U256::default(),
			gas_limit: U256::default(),
			extra_data: Default::default(),
			timestamp: Default::default(),
			difficulty: U256::default(),
			mix_hash: Default::default(),
			nonce: Default::default(),
		};
		Ok(block_header)
	}

	impl From<BlockHeader> for EthNumberBloom {
		fn from(block_header: BlockHeader) -> Self {
			EthNumberBloom {
				block_number: block_header.number.unwrap(),
				logs_bloom: block_header.logs_bloom,
				base_fee_per_gas: block_header.base_fee_per_gas.unwrap(),
			}
		}
	}

	#[tokio::test]
	async fn returns_none_when_none_in_inner_no_safety() {
		let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![]);

		let logger = new_test_logger();
		let mut stream = safe_ws_head_stream(header_stream, 0, &logger);

		assert!(stream.next().await.is_none());
	}

	#[tokio::test]
	async fn returns_none_when_none_in_inner_with_safety() {
		let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![]);

		let logger = new_test_logger();
		let mut stream = safe_ws_head_stream(header_stream, 4, &logger);

		assert!(stream.next().await.is_none());
	}

	#[tokio::test]
	async fn returns_none_when_some_in_inner_when_safety() {
		let header_stream =
			stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![block_header(1, 0)]);

		let logger = new_test_logger();
		let mut stream = safe_ws_head_stream(header_stream, 4, &logger);

		assert!(stream.next().await.is_none());
	}

	#[tokio::test]
	async fn returns_one_when_one_in_inner_but_no_more_when_no_safety() {
		let first_block = block_header(1, 0);
		let header_stream =
			stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![first_block.clone()]);

		let logger = new_test_logger();
		let mut stream = safe_ws_head_stream(header_stream, 0, &logger);

		assert_eq!(stream.next().await.unwrap(), first_block.unwrap().into());
		assert!(stream.next().await.is_none());
	}

	#[tokio::test]
	async fn returns_two_when_three_in_inner_with_one_safety_then_no_more() {
		let first_block = block_header(1, 0);
		let second_block = block_header(2, 1);
		let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
			first_block.clone(),
			second_block.clone(),
			block_header(3, 2),
		]);

		let logger = new_test_logger();
		let mut stream = safe_ws_head_stream(header_stream, 1, &logger);

		assert_eq!(stream.next().await.unwrap(), first_block.unwrap().into());
		assert_eq!(stream.next().await.unwrap(), second_block.unwrap().into());
		assert!(stream.next().await.is_none());
	}

	#[tokio::test]
	async fn returns_reorgs_of_depth_1_blocks_if_in_inner_when_no_safety() {
		// NB: Same block number, different blocks. Our node saw two blocks at the same height, so
		// returns them both
		let first_block = block_header(1, 0);
		let first_block_prime = block_header(2, 0);
		let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
			first_block.clone(),
			first_block_prime.clone(),
		]);

		let logger = new_test_logger();
		let mut stream = safe_ws_head_stream(header_stream, 0, &logger);

		assert_eq!(stream.next().await.unwrap(), first_block.clone().unwrap().into());
		assert_eq!(stream.next().await.unwrap(), first_block_prime.unwrap().into());
		assert!(stream.next().await.is_none());
	}

	#[tokio::test]
	async fn handles_reogs_depth_1_blocks_when_safety() {
		let first_block = block_header(1, 0);
		let first_block_prime = block_header(11, 0);
		let second_block_prime = block_header(2, 1);
		let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
			first_block.clone(),
			first_block_prime.clone(),
			second_block_prime.clone(),
			block_header(2, 2),
		]);

		let logger = new_test_logger();
		let mut stream = safe_ws_head_stream(header_stream, 1, &logger);

		assert_eq!(stream.next().await.unwrap(), first_block_prime.unwrap().into());
		assert_eq!(stream.next().await.unwrap(), second_block_prime.unwrap().into());
		assert!(stream.next().await.is_none());
	}

	#[tokio::test]
	async fn safe_stream_when_reorg_of_depth_below_safety() {
		let first_block = block_header(1, 10);
		let second_block = block_header(2, 11);
		let first_block_prime = block_header(11, 10);
		let second_block_prime = block_header(21, 11);
		let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
			first_block.clone(),
			second_block.clone(),
			first_block_prime.clone(),
			second_block_prime.clone(),
			block_header(2, 12),
		]);

		let logger = new_test_logger();
		let mut stream = safe_ws_head_stream(header_stream, 2, &logger);

		assert_eq!(stream.next().await.unwrap(), first_block_prime.unwrap().into());
		assert!(stream.next().await.is_none());
	}

	#[tokio::test]
	async fn safe_stream_terminates_when_input_stream_skips_into_future() {
		let first_block = block_header(1, 11);

		let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
			first_block.clone(),
			block_header(2, 12),
			block_header(4, 14),
		]);

		let logger = new_test_logger();
		let mut stream = safe_ws_head_stream(header_stream, 1, &logger);

		assert_eq!(stream.next().await.unwrap(), first_block.unwrap().into());

		assert!(stream.next().await.is_none());
	}

	#[tokio::test]
	async fn safe_stream_terminates_when_reorg_further_back_than_safety_margin() {
		let first_block = block_header(1, 11);
		let second_block = block_header(2, 12);
		let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
			first_block.clone(),
			second_block.clone(),
			block_header(4, 13),
			block_header(6, 14),
			block_header(41, 4),
			block_header(51, 5),
			block_header(61, 6),
		]);

		let logger = new_test_logger();
		let mut stream = safe_ws_head_stream(header_stream, 2, &logger);

		assert_eq!(stream.next().await.unwrap(), first_block.unwrap().into());

		assert_eq!(stream.next().await.unwrap(), second_block.unwrap().into());

		assert!(stream.next().await.is_none());
	}

	// The stream continues after a series of blocks, where we error on expected X, and then fetch X
	// e.g. if the stream is Ok(10), Ok(11), Err, Ok(12) - will work fine
	#[tokio::test]
	async fn safe_stream_terminates_after_on_error_block_no_safety() {
		let first_block = block_header(1, 11);
		let second_block_after_err = block_header(2, 12);

		let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
			first_block.clone(),
			Err(web3::Error::Internal),
			second_block_after_err.clone(),
		]);

		let logger = new_test_logger();
		let mut stream = safe_ws_head_stream(header_stream, 0, &logger);

		assert_eq!(stream.next().await.unwrap(), first_block.unwrap().into());

		assert!(stream.next().await.is_none());
	}

	#[tokio::test]
	async fn safe_stream_terminates_on_error_block_with_safety() {
		let first_block = block_header(1, 11);
		let second_block_after_err = block_header(2, 12);

		let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
			first_block.clone(),
			Err(web3::Error::Internal),
			second_block_after_err.clone(),
			block_header(3, 13),
			block_header(4, 14),
		]);

		let logger = new_test_logger();
		let mut stream = safe_ws_head_stream(header_stream, 2, &logger);

		// terminate before getting a block, since the first block is not beyond our safety margin
		assert!(stream.next().await.is_none());
	}

	// Ensure we return the errors when we don't have a block number,
	// in the same way as the error of pulling the header from the ws stream itself
	#[tokio::test]
	async fn safe_stream_terminates_when_error_in_header_with_safety() {
		let first_block = block_header(1, 11);
		let mut second_block = block_header(1, 11).unwrap();
		second_block.number = None;

		let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
			first_block.clone(),
			Ok(second_block.clone()),
		]);

		let logger = new_test_logger();
		let mut stream = safe_ws_head_stream(header_stream, 1, &logger);

		assert!(stream.next().await.is_none());
	}
}
