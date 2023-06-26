use anyhow::Result;
use ethers::prelude::*;
use std::sync::Arc;

abigen!(Vault, "eth-contract-abis/perseverance-rc17/IVault.json");

pub struct VaultRpc {
	inner_vault: Vault<Provider<Http>>,
}

impl VaultRpc {
	pub fn new(provider: Arc<Provider<Http>>, vault_contract_address: H160) -> Self {
		let inner_vault = Vault::new(vault_contract_address, provider);
		Self { inner_vault }
	}
}

#[async_trait::async_trait]
pub trait VaultApi {
	async fn fetched_native_events(&self, block_hash: H256) -> Result<Vec<FetchedNativeFilter>>;
}

#[async_trait::async_trait]
impl VaultApi for VaultRpc {
	async fn fetched_native_events(&self, block_hash: H256) -> Result<Vec<FetchedNativeFilter>> {
		let fetched_native_events =
			self.inner_vault.event::<FetchedNativeFilter>().at_block_hash(block_hash);

		Ok(fetched_native_events.query().await?)
	}
}
