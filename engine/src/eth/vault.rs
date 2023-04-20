use std::sync::Arc;

use async_trait::async_trait;
use cf_chains::address::EncodedAddress;
use cf_primitives::{Asset, EpochIndex, EthereumAddress};
use tracing::info;
use web3::{
	ethabi::{self, RawLog},
	types::{H160, H256},
};

use crate::{
	eth::EventParseError,
	state_chain_observer::client::{
		base_rpc_api::{BaseRpcClient, RawRpcApi},
		extrinsic_api::ExtrinsicApi,
		StateChainClient,
	},
};

use super::{
	event::Event, rpc::EthRpcApi, utils::decode_log_param, BlockWithItems, EthContractWitnesser,
	SignatureAndEvent,
};

use anyhow::{bail, Result};

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
		amount: u128,
		sender: ethabi::Address,
	},
	SwapToken {
		destination_chain: u32,
		destination_address: web3::types::Bytes,
		destination_token: u32,
		source_token: ethabi::Address,
		amount: u128,
		sender: ethabi::Address,
	},
	TransferNativeFailed {
		recipient: ethabi::Address,
		amount: u128,
	},
	TransferTokenFailed {
		recipient: ethabi::Address,
		amount: u128,
		token: ethabi::Address,
		reason: web3::types::Bytes,
	},
	XCallNative {
		destination_chain: u32,
		destination_address: web3::types::Bytes,
		destination_token: u32,
		amount: u128,
		sender: ethabi::Address,
		message: web3::types::Bytes,
		gas_amount: u128,
		refund_address: web3::types::Bytes,
	},
	XCallToken {
		destination_chain: u32,
		destination_address: web3::types::Bytes,
		destination_token: u32,
		source_token: ethabi::Address,
		amount: u128,
		sender: ethabi::Address,
		message: web3::types::Bytes,
		gas_amount: u128,
		refund_address: web3::types::Bytes,
	},
	AddGasNative {
		swap_id: [u8; 32],
		amount: u128,
	},
	AddGasToken {
		swap_id: [u8; 32],
		amount: u128,
		token: ethabi::Address,
	},
}

#[async_trait]
pub trait EthAssetApi {
	async fn asset(&self, token_address: EthereumAddress) -> Result<Option<Asset>>;
}

#[async_trait]
impl<RawRpcClient: RawRpcApi + Send + Sync + 'static> EthAssetApi
	for StateChainClient<BaseRpcClient<RawRpcClient>>
{
	async fn asset(&self, token_address: EthereumAddress) -> Result<Option<Asset>> {
		self.base_rpc_client
			.raw_rpc_client
			.cf_eth_asset(None, token_address)
			.await
			.map_err(Into::into)
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
		StateChainClient: ExtrinsicApi + EthAssetApi + Send + Sync,
	{
		for event in block.block_items {
			info!("Handling event: {event}");
			let call = match event.event_parameters {
				VaultEvent::SwapNative {
					destination_chain,
					destination_address,
					destination_token,
					amount,
					sender: _,
				} => pallet_cf_swapping::Call::schedule_swap_by_witnesser {
					from: Asset::Eth,
					to: Asset::try_from(destination_token).map_err(anyhow::Error::msg)?,
					ingress_amount: amount,
					egress_address: EncodedAddress::from_chain_bytes(
						destination_chain.try_into().map_err(anyhow::Error::msg)?,
						destination_address.0,
					),
				},
				VaultEvent::SwapToken {
					destination_chain,
					destination_address,
					destination_token,
					source_token,
					amount,
					sender: _,
				} => pallet_cf_swapping::Call::schedule_swap_by_witnesser {
					from: state_chain_client
						.asset(source_token.0)
						.await
						.map_err(anyhow::Error::msg)?
						.ok_or(anyhow::anyhow!("Unknown ETH token sent from the contract"))?,
					to: Asset::try_from(destination_token).map_err(anyhow::Error::msg)?,
					ingress_amount: amount,
					egress_address: EncodedAddress::from_chain_bytes(
						destination_chain.try_into().map_err(anyhow::Error::msg)?,
						destination_address.0,
					),
				},
				_ => todo!("handle the rest"),
			};

			let _result = state_chain_client
				.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
					call: Box::new(call.into()),
					epoch_index: epoch,
				})
				.await;
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
						amount: decode_log_param::<ethabi::Uint>(&log, "amount")?
							.try_into()
							.expect("SwapNative amount should fit into u128"),
						sender: decode_log_param(&log, "sender")?,
					}
				} else if event_signature == swap_token.signature {
					let log = swap_token.event.parse_log(raw_log)?;
					VaultEvent::SwapToken {
						destination_chain: decode_log_param(&log, "dstChain")?,
						destination_address: decode_log_param(&log, "dstAddress")?,
						destination_token: decode_log_param(&log, "dstToken")?,
						source_token: decode_log_param(&log, "srcToken")?,
						amount: decode_log_param::<ethabi::Uint>(&log, "amount")?
							.try_into()
							.expect("SwapToken amount should fit into u128"),
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
						amount: decode_log_param::<ethabi::Uint>(&log, "amount")?
							.try_into()
							.expect("TransferTokenFailed amount should fit into u128"),
						token: decode_log_param(&log, "token")?,
						reason: decode_log_param(&log, "reason")?,
					}
				} else if event_signature == xcall_native.signature {
					let log = xcall_native.event.parse_log(raw_log)?;
					VaultEvent::XCallNative {
						destination_chain: decode_log_param(&log, "dstChain")?,
						destination_address: decode_log_param(&log, "dstAddress")?,
						destination_token: decode_log_param(&log, "dstToken")?,
						amount: decode_log_param::<ethabi::Uint>(&log, "amount")?
							.try_into()
							.expect("XCallNative amount should fit into u128"),
						sender: decode_log_param(&log, "sender")?,
						message: decode_log_param(&log, "message")?,
						gas_amount: decode_log_param(&log, "gasAmount")?,
						refund_address: decode_log_param(&log, "refundAddress")?,
					}
				} else if event_signature == xcall_token.signature {
					let log = xcall_token.event.parse_log(raw_log)?;
					VaultEvent::XCallToken {
						destination_chain: decode_log_param(&log, "dstChain")?,
						destination_address: decode_log_param(&log, "dstAddress")?,
						destination_token: decode_log_param(&log, "dstToken")?,
						source_token: decode_log_param(&log, "srcToken")?,
						amount: decode_log_param::<ethabi::Uint>(&log, "amount")?
							.try_into()
							.expect("XCallToken amount should fit into u128"),
						sender: decode_log_param(&log, "sender")?,
						message: decode_log_param(&log, "message")?,
						gas_amount: decode_log_param(&log, "gasAmount")?,
						refund_address: decode_log_param(&log, "refundAddress")?,
					}
				} else if event_signature == add_gas_token.signature {
					let log = add_gas_token.event.parse_log(raw_log)?;
					VaultEvent::AddGasToken {
						swap_id: decode_log_param(&log, "swapID")?,
						amount: decode_log_param::<ethabi::Uint>(&log, "amount")?
							.try_into()
							.expect("AddGasToken amount should fit into u128"),
						token: decode_log_param(&log, "token")?,
					}
				} else if event_signature == add_gas_native.signature {
					let log = add_gas_native.event.parse_log(raw_log)?;
					VaultEvent::AddGasNative {
						swap_id: decode_log_param(&log, "swapID")?,
						amount: decode_log_param::<ethabi::Uint>(&log, "amount")?
							.try_into()
							.expect("AddGasNative amount should fit into u128"),
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
			contract: ethabi::Contract::load(std::include_bytes!("abis/Vault.json").as_ref())
				.unwrap(),
		}
	}
}
