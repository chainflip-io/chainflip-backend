use crate::{
	btc::{
		retry_rpc::BtcRetryRpcClient,
		rpc::{BlockHeader, BtcRpcApi, VerboseBlock},
	},
	caching_client::CachingClient,
};
use bitcoin::{BlockHash, Txid};
use cf_chains::btc::{BlockNumber, BtcAmount};
use std::{fmt::Debug, hash::Hash};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RequestKey {
	BlockHash(BlockNumber),
	Block(BlockHash),
	SendRawTransaction(Vec<u8>),
	NextBlockFeeRate(),
	AvgBlockFeeRate(BlockHash),
	BlockHeader(BlockHash),
	BestBlockHeader(),
	BestBlockHash(),
}

pub type BtcCachingClient = CachingClient<RequestKey, BtcRetryRpcClient>;

#[async_trait::async_trait]
impl BtcRpcApi for BtcCachingClient {
	async fn block(&self, block_hash: BlockHash) -> anyhow::Result<VerboseBlock> {
		let key = RequestKey::Block(block_hash);
		self.get::<VerboseBlock>(
			Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.block(block_hash).await })
			}),
			key,
		)
		.await
	}

	async fn block_hash(&self, block_number: BlockNumber) -> anyhow::Result<BlockHash> {
		let key = RequestKey::BlockHash(block_number);
		self.get::<BlockHash>(
			Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.block_hash(block_number).await })
			}),
			key,
		)
		.await
	}

	async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> anyhow::Result<Txid> {
		let key = RequestKey::SendRawTransaction(transaction_bytes.clone());
		self.get::<Txid>(
			Box::pin(move |client| {
				let transaction_bytes = transaction_bytes.clone();
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.send_raw_transaction(transaction_bytes).await })
			}),
			key,
		)
		.await
	}

	async fn next_block_fee_rate(&self) -> anyhow::Result<Option<BtcAmount>> {
		let key = RequestKey::NextBlockFeeRate();
		self.get::<Option<BtcAmount>>(
			Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.next_block_fee_rate().await })
			}),
			key,
		)
		.await
	}

	async fn average_block_fee_rate(&self, block_hash: BlockHash) -> anyhow::Result<BtcAmount> {
		let key = RequestKey::AvgBlockFeeRate(block_hash);
		self.get::<BtcAmount>(
			Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.average_block_fee_rate(block_hash).await })
			}),
			key,
		)
		.await
	}

	async fn best_block_hash(&self) -> anyhow::Result<BlockHash> {
		let key = RequestKey::BestBlockHash();
		self.get::<BlockHash>(
			Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.best_block_hash().await })
			}),
			key,
		)
		.await
	}

	async fn block_header(&self, block_hash: BlockHash) -> anyhow::Result<BlockHeader> {
		let key = RequestKey::BlockHeader(block_hash);
		self.get::<BlockHeader>(
			Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.block_header(block_hash).await })
			}),
			key,
		)
		.await
	}
}
