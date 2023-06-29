use std::sync::Arc;

use async_trait::async_trait;
use cf_chains::{address::EncodedAddress, include_abi_bytes, CcmDepositMetadata};
use cf_primitives::{Asset, EpochIndex, EthereumAddress, ForeignChain};
use tracing::info;
use web3::{
	ethabi::{self, RawLog, Uint},
	types::{H160, H256},
};

use crate::{
	eth::{core_h160, EventParseError},
	state_chain_observer::client::{
		base_rpc_api::{BaseRpcClient, RawRpcApi},
		extrinsic_api::signed::SignedExtrinsicApi,
		StateChainClient,
	},
};

use super::{
	event::Event, rpc::EthRpcApi, utils::decode_log_param, BlockWithItems, EthContractWitnesser,
	SignatureAndEvent,
};

use anyhow::{anyhow, bail, Result};

pub struct Vault {
	pub deployed_address: H160,
	contract: ethabi::Contract,
}

// The following events need to reflect the events emitted in the Vault contract:
// https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/Vault.sol
#[derive(Debug, PartialEq, Eq)]
pub enum VaultEvent {
	CommunityGuardDisabled {
		community_guard_disabled: bool,
	},
	Suspended {
		suspended: bool,
	},
	UpdatedKeyManager {
		key_manager: ethabi::Address,
	},
	SwapNative {
		destination_chain: u32,
		destination_address: web3::types::Bytes,
		destination_token: u32,
		amount: Uint,
		sender: ethabi::Address,
	},
	SwapToken {
		destination_chain: u32,
		destination_address: web3::types::Bytes,
		destination_token: u32,
		source_token: ethabi::Address,
		amount: Uint,
		sender: ethabi::Address,
	},
	TransferNativeFailed {
		recipient: ethabi::Address,
		amount: Uint,
	},
	TransferTokenFailed {
		recipient: ethabi::Address,
		amount: Uint,
		token: ethabi::Address,
		reason: web3::types::Bytes,
	},
	XCallNative {
		destination_chain: u32,
		destination_address: web3::types::Bytes,
		destination_token: u32,
		amount: Uint,
		sender: ethabi::Address,
		message: web3::types::Bytes,
		gas_amount: Uint,
		cf_parameters: web3::types::Bytes,
	},
	XCallToken {
		destination_chain: u32,
		destination_address: web3::types::Bytes,
		destination_token: u32,
		source_token: ethabi::Address,
		amount: Uint,
		sender: ethabi::Address,
		message: web3::types::Bytes,
		gas_amount: Uint,
		cf_parameters: web3::types::Bytes,
	},
	AddGasNative {
		swap_id: [u8; 32],
		amount: Uint,
	},
	AddGasToken {
		swap_id: [u8; 32],
		amount: Uint,
		token: ethabi::Address,
	},
	ExecuteActionsFailed {
		multicall_address: ethabi::Address,
		amount: Uint,
		token: ethabi::Address,
		reason: web3::types::Bytes,
	},
}

#[async_trait]
pub trait EthAssetApi {
	async fn asset(&self, token_address: EthereumAddress) -> Result<Option<Asset>>;
}

#[async_trait]
impl<RawRpcClient: RawRpcApi + Send + Sync + 'static, SignedExtrinsicClient: Send + Sync>
	EthAssetApi for StateChainClient<SignedExtrinsicClient, BaseRpcClient<RawRpcClient>>
{
	async fn asset(&self, token_address: EthereumAddress) -> Result<Option<Asset>> {
		self.base_rpc_client
			.raw_rpc_client
			.cf_eth_asset(None, token_address)
			.await
			.map_err(Into::into)
	}
}

enum CallFromEventError {
	Network(anyhow::Error),
	Decode(String),
}

async fn call_from_event<StateChainClient>(
	event: Event<VaultEvent>,
	state_chain_client: Arc<StateChainClient>,
) -> Result<pallet_cf_swapping::Call<state_chain_runtime::Runtime>, CallFromEventError>
where
	StateChainClient: EthAssetApi,
{
	fn into_encoded_address_or_ignore(
		chain: ForeignChain,
		bytes: Vec<u8>,
	) -> Result<EncodedAddress, CallFromEventError> {
		EncodedAddress::from_chain_bytes(chain, bytes).map_err(|e| {
			CallFromEventError::Decode(format!("Failed to convert into EncodedAddress: {e}"))
		})
	}

	fn into_or_ignore<Primitive: std::fmt::Debug + TryInto<CfType> + Copy, CfType>(
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
		VaultEvent::SwapNative {
			destination_chain,
			destination_address,
			destination_token,
			amount,
			sender: _,
		} => Ok(
			pallet_cf_swapping::Call::<state_chain_runtime::Runtime>::schedule_swap_from_contract {
				from: Asset::Eth,
				to: into_or_ignore(destination_token)?,
				deposit_amount: into_or_ignore(amount)?,
				destination_address: into_encoded_address_or_ignore(
					into_or_ignore(destination_chain)?,
					destination_address.0,
				)?,
				tx_hash: event.tx_hash.into(),
			},
		),
		VaultEvent::SwapToken {
			destination_chain,
			destination_address,
			destination_token,
			source_token,
			amount,
			sender: _,
		} => Ok(pallet_cf_swapping::Call::schedule_swap_from_contract {
			from: state_chain_client
				.asset(source_token.0)
				.await
				.map_err(|e| CallFromEventError::Network(anyhow!(e)))?
				.ok_or(CallFromEventError::Decode(format!(
					"Source token {source_token} not found"
				)))?,
			to: into_or_ignore(destination_token)?,
			deposit_amount: into_or_ignore(amount)?,
			destination_address: into_encoded_address_or_ignore(
				into_or_ignore(destination_chain)?,
				destination_address.0,
			)?,
			tx_hash: event.tx_hash.into(),
		}),
		VaultEvent::XCallNative {
			destination_chain,
			destination_address,
			destination_token,
			amount,
			sender,
			message,
			gas_amount,
			cf_parameters,
		} => Ok(pallet_cf_swapping::Call::ccm_deposit {
			source_asset: Asset::Eth,
			destination_asset: into_or_ignore(destination_token)?,
			deposit_amount: into_or_ignore(amount)?,
			destination_address: into_encoded_address_or_ignore(
				into_or_ignore(destination_chain)?,
				destination_address.0,
			)?,
			message_metadata: CcmDepositMetadata {
				message: message.0,
				gas_budget: into_or_ignore(gas_amount)?,
				cf_parameters: cf_parameters.0.to_vec(),
				source_address: core_h160(sender).into(),
			},
		}),
		VaultEvent::XCallToken {
			destination_chain,
			destination_address,
			destination_token,
			source_token,
			amount,
			sender,
			message,
			gas_amount,
			cf_parameters,
		} => Ok(pallet_cf_swapping::Call::ccm_deposit {
			source_asset: state_chain_client
				.asset(source_token.0)
				.await
				.map_err(|e| CallFromEventError::Network(anyhow!(e)))?
				.ok_or(CallFromEventError::Decode(format!(
					"Source token {source_token} not found"
				)))?,
			destination_asset: into_or_ignore(destination_token)?,
			deposit_amount: into_or_ignore(amount)?,
			destination_address: into_encoded_address_or_ignore(
				into_or_ignore(destination_chain)?,
				destination_address.0,
			)?,
			message_metadata: CcmDepositMetadata {
				message: message.0,
				gas_budget: into_or_ignore(gas_amount)?,
				cf_parameters: cf_parameters.0.to_vec(),
				source_address: core_h160(sender).into(),
			},
		}),
		unhandled_event => Err(CallFromEventError::Decode(format!(
			"Unhandled vault contract event: {unhandled_event:?}"
		))),
	}
}

#[async_trait]
impl EthContractWitnesser for Vault {
	type EventParameters = VaultEvent;

	fn contract_name(&self) -> String {
		"Vault".to_string()
	}

	async fn handle_block_events<StateChainClient, EthRpcClient>(
		&mut self,
		epoch: EpochIndex,
		_block_number: u64,
		block: BlockWithItems<Event<Self::EventParameters>>,
		state_chain_client: Arc<StateChainClient>,
		_eth_rpc: &EthRpcClient,
	) -> Result<()>
	where
		EthRpcClient: EthRpcApi + Sync + Send,
		StateChainClient: SignedExtrinsicApi + EthAssetApi + Send + Sync,
	{
		for event in block.block_items {
			info!("Handling event: {event}");

			match call_from_event(event, state_chain_client.clone()).await {
				Ok(call) => {
					state_chain_client
						.finalize_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(call.into()),
							epoch_index: epoch,
						})
						.await;
				},
				Err(CallFromEventError::Network(err)) => return Err(err),
				Err(CallFromEventError::Decode(message)) => {
					tracing::warn!("Ignoring event: {message}");
					continue
				},
			}
		}
		Ok(())
	}

	fn decode_log_closure(&self) -> Result<super::DecodeLogClosure<Self::EventParameters>> {
		let community_guard_disabled =
			SignatureAndEvent::new(&self.contract, "CommunityGuardDisabled")?;
		let suspended = SignatureAndEvent::new(&self.contract, "Suspended")?;
		let updated_key_manager = SignatureAndEvent::new(&self.contract, "UpdatedKeyManager")?;
		let swap_native = SignatureAndEvent::new(&self.contract, "SwapNative")?;
		let swap_token = SignatureAndEvent::new(&self.contract, "SwapToken")?;
		let transfer_native_failed =
			SignatureAndEvent::new(&self.contract, "TransferNativeFailed")?;
		let transfer_token_failed = SignatureAndEvent::new(&self.contract, "TransferTokenFailed")?;
		let xcall_native = SignatureAndEvent::new(&self.contract, "XCallNative")?;
		let xcall_token = SignatureAndEvent::new(&self.contract, "XCallToken")?;
		let add_gas_native = SignatureAndEvent::new(&self.contract, "AddGasNative")?;
		let add_gas_token = SignatureAndEvent::new(&self.contract, "AddGasToken")?;
		let execute_actions_failed =
			SignatureAndEvent::new(&self.contract, "ExecuteActionsFailed")?;

		Ok(Box::new(
			move |event_signature: H256, raw_log: RawLog| -> Result<Self::EventParameters> {
				Ok(if event_signature == community_guard_disabled.signature {
					let log = community_guard_disabled.event.parse_log(raw_log)?;
					VaultEvent::CommunityGuardDisabled {
						community_guard_disabled: decode_log_param(&log, "communityGuardDisabled")?,
					}
				} else if event_signature == suspended.signature {
					let log = suspended.event.parse_log(raw_log)?;
					VaultEvent::Suspended { suspended: decode_log_param(&log, "suspended")? }
				} else if event_signature == updated_key_manager.signature {
					let log = updated_key_manager.event.parse_log(raw_log)?;
					VaultEvent::UpdatedKeyManager {
						key_manager: decode_log_param(&log, "keyManager")?,
					}
				} else if event_signature == swap_native.signature {
					let log = swap_native.event.parse_log(raw_log)?;
					VaultEvent::SwapNative {
						destination_chain: decode_log_param(&log, "dstChain")?,
						destination_address: decode_log_param(&log, "dstAddress")?,
						destination_token: decode_log_param(&log, "dstToken")?,
						amount: decode_log_param(&log, "amount")?,
						sender: decode_log_param(&log, "sender")?,
					}
				} else if event_signature == swap_token.signature {
					let log = swap_token.event.parse_log(raw_log)?;
					VaultEvent::SwapToken {
						destination_chain: decode_log_param(&log, "dstChain")?,
						destination_address: decode_log_param(&log, "dstAddress")?,
						destination_token: decode_log_param(&log, "dstToken")?,
						source_token: decode_log_param(&log, "srcToken")?,
						amount: decode_log_param(&log, "amount")?,
						sender: decode_log_param(&log, "sender")?,
					}
				} else if event_signature == transfer_native_failed.signature {
					let log = transfer_native_failed.event.parse_log(raw_log)?;
					VaultEvent::TransferNativeFailed {
						recipient: decode_log_param(&log, "recipient")?,
						amount: decode_log_param(&log, "amount")?,
					}
				} else if event_signature == transfer_token_failed.signature {
					let log = transfer_token_failed.event.parse_log(raw_log)?;
					VaultEvent::TransferTokenFailed {
						recipient: decode_log_param(&log, "recipient")?,
						amount: decode_log_param(&log, "amount")?,
						token: decode_log_param(&log, "token")?,
						reason: decode_log_param(&log, "reason")?,
					}
				} else if event_signature == xcall_native.signature {
					let log = xcall_native.event.parse_log(raw_log)?;
					VaultEvent::XCallNative {
						destination_chain: decode_log_param(&log, "dstChain")?,
						destination_address: decode_log_param(&log, "dstAddress")?,
						destination_token: decode_log_param(&log, "dstToken")?,
						amount: decode_log_param(&log, "amount")?,
						sender: decode_log_param(&log, "sender")?,
						message: decode_log_param(&log, "message")?,
						gas_amount: decode_log_param(&log, "gasAmount")?,
						cf_parameters: decode_log_param(&log, "cfParameters")?,
					}
				} else if event_signature == xcall_token.signature {
					let log = xcall_token.event.parse_log(raw_log)?;
					VaultEvent::XCallToken {
						destination_chain: decode_log_param(&log, "dstChain")?,
						destination_address: decode_log_param(&log, "dstAddress")?,
						destination_token: decode_log_param(&log, "dstToken")?,
						source_token: decode_log_param(&log, "srcToken")?,
						amount: decode_log_param(&log, "amount")?,
						sender: decode_log_param(&log, "sender")?,
						message: decode_log_param(&log, "message")?,
						gas_amount: decode_log_param(&log, "gasAmount")?,
						cf_parameters: decode_log_param(&log, "cfParameters")?,
					}
				} else if event_signature == add_gas_token.signature {
					let log = add_gas_token.event.parse_log(raw_log)?;
					VaultEvent::AddGasToken {
						swap_id: decode_log_param(&log, "swapID")?,
						amount: decode_log_param(&log, "amount")?,
						token: decode_log_param(&log, "token")?,
					}
				} else if event_signature == add_gas_native.signature {
					let log = add_gas_native.event.parse_log(raw_log)?;
					VaultEvent::AddGasNative {
						swap_id: decode_log_param(&log, "swapID")?,
						amount: decode_log_param(&log, "amount")?,
					}
				} else if event_signature == execute_actions_failed.signature {
					let log = execute_actions_failed.event.parse_log(raw_log)?;
					VaultEvent::ExecuteActionsFailed {
						multicall_address: decode_log_param(&log, "multicallAddress")?,
						amount: decode_log_param(&log, "amount")?,
						token: decode_log_param(&log, "token")?,
						reason: decode_log_param(&log, "reason")?,
					}
				} else {
					bail!(EventParseError::UnexpectedEvent(event_signature))
				})
			},
		))
	}

	fn contract_address(&self) -> H160 {
		self.deployed_address
	}
}

impl Vault {
	pub fn new(deployed_address: H160) -> Self {
		Self {
			deployed_address,
			contract: ethabi::Contract::load(include_abi_bytes!(IVault)).unwrap(),
		}
	}
}
