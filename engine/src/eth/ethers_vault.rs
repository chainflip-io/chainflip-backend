use super::event::Event;
use anyhow::{anyhow, Result};
use cf_chains::{address::EncodedAddress, CcmDepositMetadata};
use cf_primitives::{Asset, ForeignChain};
use ethers::prelude::*;
use std::sync::Arc;

use super::vault::EthAssetApi;

abigen!(Vault, "eth-contract-abis/perseverance-rc17/IVault.json");

pub struct VaultRpc<T> {
	inner_vault: Vault<Provider<T>>,
}

impl<T: JsonRpcClient> VaultRpc<T> {
	pub fn new(provider: Arc<Provider<T>>, vault_contract_address: H160) -> Self {
		let inner_vault = Vault::new(vault_contract_address, provider);
		Self { inner_vault }
	}
}

#[async_trait::async_trait]
pub trait VaultApi {
	async fn fetched_native_events(&self, block_hash: H256) -> Result<Vec<FetchedNativeFilter>>;
}

#[async_trait::async_trait]
impl<T: JsonRpcClient + 'static> VaultApi for VaultRpc<T> {
	async fn fetched_native_events(&self, block_hash: H256) -> Result<Vec<FetchedNativeFilter>> {
		let fetched_native_events =
			self.inner_vault.event::<FetchedNativeFilter>().at_block_hash(block_hash);

		Ok(fetched_native_events.query().await?)
	}
}

#[allow(unused)]
pub enum CallFromEventError {
	Network(anyhow::Error),
	Decode(String),
}

pub async fn call_from_event<StateChainClient>(
	event: Event<VaultEvents>,
	state_chain_client: Arc<StateChainClient>,
) -> Result<pallet_cf_swapping::Call<state_chain_runtime::Runtime>, CallFromEventError>
where
	StateChainClient: EthAssetApi,
{
	fn try_into_encoded_address(
		chain: ForeignChain,
		bytes: Vec<u8>,
	) -> Result<EncodedAddress, CallFromEventError> {
		EncodedAddress::from_chain_bytes(chain, bytes).map_err(|e| {
			CallFromEventError::Decode(format!("Failed to convert into EncodedAddress: {e}"))
		})
	}

	fn try_into_primitive<Primitive: std::fmt::Debug + TryInto<CfType> + Copy, CfType>(
		from: Primitive,
	) -> Result<CfType, CallFromEventError>
	where
		<Primitive as TryInto<CfType>>::Error: std::fmt::Display,
	{
		from.try_into().map_err(|err| {
			CallFromEventError::Decode(format!(
				"Failed to convert into {:?}: {err}",
				std::any::type_name::<CfType>(),
			))
		})
	}

	match event.event_parameters {
		VaultEvents::SwapNativeFilter(SwapNativeFilter {
			dst_chain,
			dst_address,
			dst_token,
			amount,
			sender: _,
			cf_parameters: _,
		}) => Ok(
			pallet_cf_swapping::Call::<state_chain_runtime::Runtime>::schedule_swap_from_contract {
				from: Asset::Eth,
				to: try_into_primitive(dst_token)?,
				deposit_amount: try_into_primitive(amount)?,
				destination_address: try_into_encoded_address(
					try_into_primitive(dst_chain)?,
					dst_address.to_vec(),
				)?,
				tx_hash: event.tx_hash.into(),
			},
		),
		VaultEvents::SwapTokenFilter(SwapTokenFilter {
			dst_chain,
			dst_address,
			dst_token,
			src_token,
			amount,
			sender: _,
			cf_parameters: _,
		}) => Ok(pallet_cf_swapping::Call::schedule_swap_from_contract {
			from: state_chain_client
				.asset(src_token.0)
				.await
				.map_err(|e| {
					CallFromEventError::Network(anyhow!(
						"Failed to retrieve from token for SwapToken call: {e}"
					))
				})?
				.ok_or(CallFromEventError::Decode(format!("Source token {src_token} not found")))?,
			to: try_into_primitive(dst_token)?,
			deposit_amount: try_into_primitive(amount)?,
			destination_address: try_into_encoded_address(
				try_into_primitive(dst_chain)?,
				dst_address.to_vec(),
			)?,
			tx_hash: event.tx_hash.into(),
		}),
		VaultEvents::XcallNativeFilter(XcallNativeFilter {
			dst_chain,
			dst_address,
			dst_token,
			amount,
			sender,
			message,
			gas_amount,
			cf_parameters,
		}) => Ok(pallet_cf_swapping::Call::ccm_deposit {
			source_asset: Asset::Eth,
			destination_asset: try_into_primitive(dst_token)?,
			deposit_amount: try_into_primitive(amount)?,
			destination_address: try_into_encoded_address(
				try_into_primitive(dst_chain)?,
				dst_address.to_vec(),
			)?,
			message_metadata: CcmDepositMetadata {
				message: message.to_vec(),
				gas_budget: try_into_primitive(gas_amount)?,
				cf_parameters: cf_parameters.0.to_vec(),
				source_address: sender.into(),
			},
		}),
		VaultEvents::XcallTokenFilter(XcallTokenFilter {
			dst_chain,
			dst_address,
			dst_token,
			src_token,
			amount,
			sender,
			message,
			gas_amount,
			cf_parameters,
		}) => Ok(pallet_cf_swapping::Call::ccm_deposit {
			source_asset: state_chain_client
				.asset(src_token.0)
				.await
				.map_err(|e| {
					CallFromEventError::Network(anyhow!(
						"Failed to retrieve From token for XCallToken call: {e}"
					))
				})?
				.ok_or(CallFromEventError::Decode(format!("Source token {src_token} not found")))?,
			destination_asset: try_into_primitive(dst_token)?,
			deposit_amount: try_into_primitive(amount)?,
			destination_address: try_into_encoded_address(
				try_into_primitive(dst_chain)?,
				dst_address.to_vec(),
			)?,
			message_metadata: CcmDepositMetadata {
				message: message.to_vec(),
				gas_budget: try_into_primitive(gas_amount)?,
				cf_parameters: cf_parameters.0.to_vec(),
				source_address: sender.into(),
			},
		}),
		unhandled_event => Err(CallFromEventError::Decode(format!(
			"Unhandled vault contract event: {unhandled_event:?}"
		))),
	}
}
