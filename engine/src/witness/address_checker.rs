use anyhow::Result;
use ethers::prelude::*;
use std::sync::Arc;

abigen!(AddressChecker, "eth-contract-abis/perseverance-rc17/IAddressChecker.json");

pub struct AddressCheckerRpc<T> {
	inner_address_checker: AddressChecker<Provider<T>>,
}

impl<T: JsonRpcClient> AddressCheckerRpc<T> {
	pub fn new(provider: Arc<Provider<T>>, address_checker_contract_address: H160) -> Self {
		let inner_address_checker = AddressChecker::new(address_checker_contract_address, provider);
		Self { inner_address_checker }
	}
}

#[async_trait::async_trait]
pub trait AddressCheckerApi {
	async fn address_states(
		&self,
		block_hash: H256,
		addresses: Vec<H160>,
	) -> Result<Vec<AddressState>>;

	async fn balances(&self, block_hash: H256, addresses: Vec<H160>) -> Result<Vec<U256>>;
}

#[async_trait::async_trait]
impl<T: JsonRpcClient + 'static> AddressCheckerApi for AddressCheckerRpc<T> {
	async fn address_states(
		&self,
		block_hash: H256,
		addresses: Vec<H160>,
	) -> Result<Vec<AddressState>> {
		Ok(self
			.inner_address_checker
			.address_states(addresses)
			.block(BlockId::Hash(block_hash))
			.call()
			.await?)
	}

	async fn balances(&self, block_hash: H256, addresses: Vec<H160>) -> Result<Vec<U256>> {
		Ok(self
			.inner_address_checker
			.native_balances(addresses)
			.block(BlockId::Hash(block_hash))
			.call()
			.await?)
	}
}
