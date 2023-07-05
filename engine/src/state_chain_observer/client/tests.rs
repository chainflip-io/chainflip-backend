use futures_util::SinkExt;
use mockall::predicate::eq;
use sp_runtime::Digest;

use super::*;
use crate::state_chain_observer::client::base_rpc_api::MockBaseRpcApi;

pub fn test_header_with_parent(number: u32, parent_hash: H256) -> Header {
	Header {
		number,
		parent_hash,
		state_root: H256::default(),
		extrinsics_root: H256::default(),
		digest: Digest { logs: Vec::new() },
	}
}

#[tokio::test]
async fn test_get_consecutive_headers() {
	let mut mock_rpc_api = MockBaseRpcApi::new();

	// For this test we are missing 2 headers
	const FROM_BLOCK: BlockNumber = 5;
	let missing_header_1 = test_header_with_parent(FROM_BLOCK + 1, test_header(FROM_BLOCK).hash());
	let missing_header_2 = test_header_with_parent(FROM_BLOCK + 2, missing_header_1.hash());

	// The client will first ask for first missing header
	let missing_header_1_clone = missing_header_1.clone();
	mock_rpc_api
		.expect_block_hash()
		.with(eq(missing_header_1_clone.number))
		.once()
		.return_once(move |_| Ok(Some(missing_header_1_clone.hash())));

	mock_rpc_api
		.expect_block_header()
		.with(eq(missing_header_1.hash()))
		.once()
		.return_once(move |_| Ok(missing_header_1));

	// Then it will ask for second missing header
	let missing_header_2_clone = missing_header_2.clone();
	mock_rpc_api
		.expect_block_hash()
		.with(eq(missing_header_2_clone.number))
		.once()
		.return_once(move |_| Ok(Some(missing_header_2_clone.hash())));

	let missing_header_2_clone = missing_header_2.clone();
	mock_rpc_api
		.expect_block_header()
		.with(eq(missing_header_2_clone.hash()))
		.once()
		.return_once(move |_| Ok(missing_header_2_clone));

	let mock_rpc_api = Arc::new(mock_rpc_api);

	// Run the function with FROM_BLOCK..FROM_BLOCK+3 and grab the block numbers returned
	let missing_block_numbers = get_consecutive_headers(
		test_header(FROM_BLOCK),
		test_header_with_parent(FROM_BLOCK + 3, missing_header_2.hash()),
		&mock_rpc_api,
	)
	.await
	.unwrap()
	.collect::<Result<Vec<Header>>>()
	.unwrap()
	.iter()
	.map(|header| header.number)
	.collect::<Vec<BlockNumber>>();

	// We expect to get the missing 2 headers followed by the last header
	assert_eq!(missing_block_numbers, vec![FROM_BLOCK + 1, FROM_BLOCK + 2, FROM_BLOCK + 3]);
}

#[tokio::test]
async fn test_get_finalized_block_header_stream() {
	use jsonrpsee::core::client::{Subscription, SubscriptionKind};

	// For this test we will send the subscription 2 headers that are not consecutive (one missing
	// header between them)
	const STARTING_BLOCK: BlockNumber = 5;
	let header_1 = test_header(STARTING_BLOCK);
	let missing_header = test_header_with_parent(STARTING_BLOCK + 1, header_1.hash());
	let header_2 = test_header_with_parent(STARTING_BLOCK + 2, missing_header.hash());
	let mut mock_rpc_api = MockBaseRpcApi::new();

	// Create the finalized block header subscription
	let (to_back_sender, _to_back_receiver) = futures::channel::mpsc::channel(10);
	let (mut sub_sender, sub_receiver) = futures::channel::mpsc::channel(10);
	let sub = Subscription::<Header>::new(
		to_back_sender,
		sub_receiver,
		SubscriptionKind::Method("🤷‍♂️".to_string()),
	);

	// The client will first subscribe to the finalized block headers
	mock_rpc_api
		.expect_subscribe_finalized_block_headers()
		.once()
		.return_once(move || Ok(sub));

	// Send the 2 headers, excluding the missing header
	for header in [&header_1, &header_2] {
		sub_sender.send(serde_json::to_value(header).unwrap()).await.unwrap();
	}

	// When the stream is polled, it should detect a gap and ask for the missing header
	let missing_header_clone = missing_header.clone();
	mock_rpc_api
		.expect_block_hash()
		.with(eq(missing_header_clone.number))
		.once()
		.return_once(move |_| Ok(Some(missing_header_clone.hash())));
	let missing_header_clone = missing_header.clone();
	mock_rpc_api
		.expect_block_header()
		.with(eq(missing_header_clone.hash()))
		.once()
		.return_once(move |_| Ok(missing_header_clone));

	let (initial_header, mut stream) =
		get_finalized_block_header_stream(Arc::new(mock_rpc_api)).await.unwrap();

	// The function should return the initial header
	assert_eq!(initial_header, header_1);
	// Then the stream should yield the next 2 headers in order, including the missing header
	assert_eq!(stream.next().await.unwrap().unwrap(), missing_header);
	assert_eq!(stream.next().await.unwrap().unwrap(), header_2);
}

#[tokio::test]
async fn test_fast_forward_finalized_stream_to_latest() {
	use jsonrpsee::core::client::{Subscription, SubscriptionKind};

	// For this test we will send the subscription 3 headers
	const STARTING_BLOCK: BlockNumber = 5;
	let header_1 = test_header(STARTING_BLOCK);
	let header_2 = test_header_with_parent(STARTING_BLOCK + 1, header_1.hash());
	let header_3 = test_header_with_parent(STARTING_BLOCK + 2, header_2.hash());
	let mut mock_rpc_api = MockBaseRpcApi::new();

	// Create the finalized block header subscription
	let (to_back_sender, _to_back_receiver) = futures::channel::mpsc::channel(10);
	let (mut sub_sender, sub_receiver) = futures::channel::mpsc::channel(10);
	let sub = Subscription::<Header>::new(
		to_back_sender,
		sub_receiver,
		SubscriptionKind::Method("🤷‍♂️".to_string()),
	);

	// The client will first subscribe to the finalized block headers
	mock_rpc_api
		.expect_subscribe_finalized_block_headers()
		.once()
		.return_once(move || Ok(sub));

	// Send all 3 headers
	for header in [&header_1, &header_2, &header_3] {
		sub_sender.send(serde_json::to_value(header).unwrap()).await.unwrap();
	}

	// The fast forward function will ask for the latest finalized block hash, we will return
	// header_3
	let header_3_clone = header_3.clone();
	mock_rpc_api
		.expect_latest_finalized_block_hash()
		.once()
		.return_once(move || Ok(header_3_clone.hash()));
	let header_3_clone = header_3.clone();
	mock_rpc_api
		.expect_block_header()
		.with(eq(header_3_clone.hash()))
		.once()
		.return_once(move |_| Ok(header_3_clone));

	let mock_rpc_api = Arc::new(mock_rpc_api);

	// The stream will start at header_1
	let (initial_header, mut stream) =
		get_finalized_block_header_stream(mock_rpc_api.clone()).await.unwrap();

	let latest_block_number =
		fast_forward_finalized_stream_to_latest(initial_header, &mut stream, &mock_rpc_api)
			.await
			.unwrap()
			.1;

	// The function should have skipped header_2 from the stream and returned header_3
	assert_eq!(latest_block_number, header_3.number);
	assert!(
		tokio::time::timeout(std::time::Duration::from_millis(1), stream.next())
			.await
			.is_err(),
		"The stream should be empty"
	);
}
