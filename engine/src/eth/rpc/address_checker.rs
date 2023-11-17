use ethers::prelude::*;

use anyhow::{Ok, Result};

use super::{EthRpcClient, EthRpcSigningClient};

abigen!(AddressChecker, "$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IAddressChecker.json");

#[async_trait::async_trait]
pub trait AddressCheckerRpcApi {
	async fn address_states(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<AddressState>>;

	async fn balances(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<U256>>;
}

#[async_trait::async_trait]
impl AddressCheckerRpcApi for EthRpcClient {
	async fn address_states(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<AddressState>> {
		Ok(AddressChecker::new(contract_address, self.provider.clone())
			.address_states(addresses)
			.block(BlockId::Hash(block_hash))
			.call()
			.await?)
	}

	async fn balances(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<U256>> {
		Ok(AddressChecker::new(contract_address, self.provider.clone())
			.native_balances(addresses)
			.block(BlockId::Hash(block_hash))
			.call()
			.await?)
	}
}

#[async_trait::async_trait]
impl AddressCheckerRpcApi for EthRpcSigningClient {
	async fn address_states(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<AddressState>> {
		self.rpc_client.address_states(block_hash, contract_address, addresses).await
	}

	async fn balances(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<U256>> {
		self.rpc_client.balances(block_hash, contract_address, addresses).await
	}
}
