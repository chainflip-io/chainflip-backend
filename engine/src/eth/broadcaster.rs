use crate::eth::EthRpcApi;
use anyhow::{Context, Result};
use ethers::{
	prelude::*,
	types::transaction::{eip2718::TypedTransaction, eip2930::AccessList},
};
use tracing::{info_span, Instrument};

#[derive(Clone)]
pub struct EthBroadcaster<EthRpc>
where
	EthRpc: EthRpcApi,
{
	eth_rpc: EthRpc,
	pub address: H160,
}

impl<EthRpc> EthBroadcaster<EthRpc>
where
	EthRpc: EthRpcApi,
{
	pub fn new(eth_rpc: EthRpc) -> Self {
		Self { address: eth_rpc.address(), eth_rpc }
	}

	// This is so we don't have to muddy the SCO tests with expectations
	// on the "address()" call when creating the broadcaster.
	#[cfg(test)]
	pub fn new_test(eth_rpc: EthRpc) -> Self {
		Self { address: Default::default(), eth_rpc }
	}

	/// Estimates gas and signs and broadcasts the transaction.
	pub async fn send(
		&self,
		// This is from the SC.
		unsigned_tx: cf_chains::eth::Transaction,
	) -> Result<TxHash> {
		async move {
			let mut transaction_request = Eip1559TransactionRequest {
				to: Some(NameOrAddress::Address(unsigned_tx.contract)),
				data: Some(unsigned_tx.data.into()),
				chain_id: Some(unsigned_tx.chain_id.into()),
				value: Some(unsigned_tx.value),
				max_fee_per_gas: unsigned_tx.max_fee_per_gas,
				max_priority_fee_per_gas: unsigned_tx.max_priority_fee_per_gas,
				gas: unsigned_tx.gas_limit,
				access_list: AccessList::default(),
				from: Some(self.address),
				nonce: None,
			};

			let estimated_gas = self
				.eth_rpc
				.estimate_gas(&TypedTransaction::Eip1559(transaction_request.clone()))
				.await
				.context("Failed to estimate gas")?;

			// increase the estimate by 50%
			transaction_request.gas = Some(
				estimated_gas
					.saturating_mul(U256::from(3u64))
					.checked_div(U256::from(2u64))
					.unwrap(),
			);

			self.eth_rpc
				.send_transaction(transaction_request.into())
				.await
				.context("Failed to send ETH transaction")
		}
		.instrument(info_span!("EthBroadcaster"))
		.await
	}
}
