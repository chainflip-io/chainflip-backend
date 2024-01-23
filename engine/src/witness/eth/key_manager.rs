use cf_chains::evm::{
	EvmCrypto, EvmTransactionMetadata, SchnorrVerificationComponents, TransactionFee,
};
use cf_primitives::EpochIndex;
use ethers::{
	prelude::abigen,
	types::{Bloom, TransactionReceipt},
};
use futures_core::Future;
use sp_core::{H160, H256};
use state_chain_runtime::PalletInstanceAlias;
use tracing::{info, trace};

use super::{
	super::common::{
		chain_source::ChainClient,
		chunked_chain_source::chunked_by_vault::{builder::ChunkedByVaultBuilder, ChunkedByVault},
	},
	contract_common::events_at_block,
};
use crate::{
	eth::retry_rpc::EthersRetryRpcApi,
	witness::common::{RuntimeCallHasChain, RuntimeHasChain},
};
use num_traits::Zero;

abigen!(KeyManager, "$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IKeyManager.json");

// This type is generated in the macro above.
//`Key(uint256,uint8)`
impl Key {
	/// 1 byte of pub_key_y_parity followed by 32 bytes of pub_key_x
	/// Equivalent to secp256k1::PublicKey.serialize()
	pub fn serialize(&self) -> [u8; 33] {
		let mut bytes: [u8; 33] = [0; 33];
		self.pub_key_x.to_big_endian(&mut bytes[1..]);
		bytes[0] = match self.pub_key_y_parity.is_zero() {
			true => 2,
			false => 3,
		};
		bytes
	}
}

use anyhow::Result;

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub fn key_manager_witnessing<
		ProcessCall,
		ProcessingFut,
		EthRpcClient: EthersRetryRpcApi + ChainClient + Clone,
	>(
		self,
		process_call: ProcessCall,
		eth_rpc: EthRpcClient,
		contract_address: H160,
	) -> ChunkedByVaultBuilder<impl ChunkedByVault>
	where
		// These are the types for EVM chains, so this adapter can be shared by all EVM chains.
		Inner: ChunkedByVault<Index = u64, Hash = H256, Data = Bloom>,
		Inner::Chain: cf_chains::Chain<
			ChainCrypto = EvmCrypto,
			ChainAccount = H160,
			TransactionFee = TransactionFee,
			TransactionMetadata = EvmTransactionMetadata,
		>,
		ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
			+ Send
			+ Sync
			+ Clone
			+ 'static,
		ProcessingFut: Future<Output = ()> + Send + 'static,
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		state_chain_runtime::RuntimeCall:
			RuntimeCallHasChain<state_chain_runtime::Runtime, Inner::Chain>,
	{
		self.then::<Result<Bloom>, _, _>(move |epoch, header| {
			let process_call = process_call.clone();
			let eth_rpc = eth_rpc.clone();
			async move {
				for event in
					events_at_block::<KeyManagerEvents, _>(header, contract_address, &eth_rpc)
						.await?
				{
					info!("Handling event: {event}");
					let call: state_chain_runtime::RuntimeCall = match event.event_parameters {
						KeyManagerEvents::AggKeySetByGovKeyFilter(AggKeySetByGovKeyFilter {
							new_agg_key,
							..
						}) => pallet_cf_vaults::Call::<
							_,
							<Inner::Chain as PalletInstanceAlias>::Instance,
						>::vault_key_rotated_externally {
							new_public_key: cf_chains::evm::AggKey::from_pubkey_compressed(
								new_agg_key.serialize(),
							),
							block_number: header.index,
							tx_id: event.tx_hash,
						}
						.into(),
						KeyManagerEvents::SignatureAcceptedFilter(SignatureAcceptedFilter {
							sig_data,
							..
						}) => {
							let TransactionReceipt {
								gas_used, effective_gas_price, from, to, ..
							} = eth_rpc.transaction_receipt(event.tx_hash).await;

							let gas_used = gas_used
								.ok_or_else(|| {
									anyhow::anyhow!(
										"No gas_used on Transaction receipt for tx_hash: {}",
										event.tx_hash
									)
								})?
								.try_into()
								.map_err(anyhow::Error::msg)?;
							let effective_gas_price = effective_gas_price
								.ok_or_else(|| {
									anyhow::anyhow!(
										"No effective_gas_price on Transaction receipt for tx_hash: {}"
									, event.tx_hash)
								})?
								.try_into()
								.map_err(anyhow::Error::msg)?;

							let transaction = eth_rpc.get_transaction(event.tx_hash).await;
							let tx_metadata = EvmTransactionMetadata {
								contract: to.expect("To have a contract"),
								max_fee_per_gas: transaction.max_fee_per_gas,
								max_priority_fee_per_gas: transaction.max_priority_fee_per_gas,
								gas_limit: Some(transaction.gas),
							};
							pallet_cf_broadcast::Call::<
								_,
								<Inner::Chain as PalletInstanceAlias>::Instance,
							>::transaction_succeeded {
								tx_out_id: SchnorrVerificationComponents {
									s: sig_data.sig.into(),
									k_times_g_address: sig_data.k_times_g_address.into(),
								},
								signer_id: from,
								tx_fee: TransactionFee { effective_gas_price, gas_used },
								tx_metadata,
							}
							.into()
						},
						KeyManagerEvents::GovernanceActionFilter(GovernanceActionFilter {
							message,
						}) => pallet_cf_governance::Call::set_whitelisted_call_hash {
							call_hash: message,
						}
						.into(),
						_ => {
							trace!("Ignoring unused event: {event}");
							continue
						},
					};
					process_call(call, epoch.index).await;
				}

				Result::Ok(header.data)
			}
		})
	}
}

#[cfg(test)]
mod tests {

	use std::{path::PathBuf, str::FromStr};

	use cf_primitives::AccountRole;
	use futures_util::FutureExt;
	use sp_core::{H160, U256};
	use utilities::task_scope::task_scope;

	use super::super::eth_source::EthSource;

	use crate::{
		eth::{retry_rpc::EthRetryRpcClient, rpc::EthRpcClient},
		settings::{NodeContainer, WsHttpEndpoints},
		state_chain_observer::client::StateChainClient,
		witness::common::{chain_source::extension::ChainSourceExt, epoch_source::EpochSource},
	};

	#[ignore = "requires connection to live network"]
	#[tokio::test]
	async fn test_key_manager_witnesser() {
		task_scope(|scope| {
			async {
				let retry_client = EthRetryRpcClient::<EthRpcClient>::new(
					scope,
					NodeContainer {
						primary: WsHttpEndpoints {
							ws_endpoint: "ws://localhost:8546".into(),
							http_endpoint: "http://localhost:8545".into(),
						},
						backup: None,
					},
					U256::from(1337u64),
				)
				.unwrap();

				let (state_chain_stream, _, state_chain_client) =
					StateChainClient::connect_with_account(
						scope,
						"ws://localhost:9944",
						PathBuf::from_str("/some/sc/key/bashful-key").unwrap().as_path(),
						AccountRole::Unregistered,
						false,
						false,
						false,
					)
					.await
					.unwrap();

				let vault_source =
					EpochSource::builder(scope, state_chain_stream, state_chain_client.clone())
						.await
						.vaults()
						.await;

				EthSource::new(retry_client.clone())
					.chunk_by_vault(vault_source, scope)
					.key_manager_witnessing(
						|call, _| async move {
							println!("Witnessed call: {:?}", call);
						},
						retry_client,
						H160::from_str("a16e02e87b7454126e5e10d957a927a7f5b5d2be").unwrap(),
					)
					.spawn(scope);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
