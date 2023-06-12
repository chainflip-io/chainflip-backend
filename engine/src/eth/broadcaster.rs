use ethers::prelude::*;
use crate::eth::EthersRpcApi;
use tracing::{info_span, Instrument};
use anyhow::{Result, Context};
use ethers::types::transaction::eip2930::AccessList;
use ethers::types::transaction::eip2718::TypedTransaction;

#[derive(Clone)]
pub struct EthBroadcaster<EthRpc>
where
	EthRpc: EthersRpcApi,
{
	eth_rpc: EthRpc,
	pub address: H160,
}

impl<EthRpc> EthBroadcaster<EthRpc>
where
	EthRpc: EthersRpcApi,
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

	pub async fn estimate_gas_and_send_transaction(
		&self,
		// This is from the SC.
		unsigned_tx: cf_chains::eth::Transaction,
	) -> Result<TxHash> {
		async move {
            let mut transaction_request =  Eip1559TransactionRequest {
				to: Some(NameOrAddress::Address(unsigned_tx.contract)),
				data: Some(unsigned_tx.data.into()),
				chain_id: Some(unsigned_tx.chain_id.into()),
				value: Some(unsigned_tx.value),
				max_fee_per_gas: unsigned_tx.max_fee_per_gas,
				max_priority_fee_per_gas: unsigned_tx.max_priority_fee_per_gas,
				gas: Some(U256::from(15_000_000u64)),
                access_list: AccessList::default(),
                from: Some(self.address),
                nonce: None,
            };

            let estimated_gas =	self.eth_rpc
                .estimate_gas(&TypedTransaction::Eip1559(transaction_request.clone()))
                .await
                .context("Failed to estimate gas")?;

			// increase the estimate by 50%
            transaction_request.gas = Some(estimated_gas
                .saturating_mul(U256::from(3u64))
                .checked_div(U256::from(2u64))
                .unwrap());

			self
				.eth_rpc
				.send_transaction(transaction_request.into())
				.await
				.context("Failed to send ETH transaction")

		}.instrument(info_span!("EthBroadcaster")).await
	}
}
