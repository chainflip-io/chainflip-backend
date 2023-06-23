use crate::{
	eth::{core_h160, core_h256, EthRpcApi},
	state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi,
};
use cf_chains::eth::{SchnorrVerificationComponents, TransactionFee};
use cf_primitives::EpochIndex;
use state_chain_runtime::EthereumInstance;
use std::sync::Arc;
use tracing::{info, trace};
use web3::{
	contract::tokens::Tokenizable,
	ethabi::{self, Token},
	types::{TransactionReceipt, H160},
};

use anyhow::{Context, Result};

use std::fmt::Debug;

use async_trait::async_trait;

use super::{event::Event, BlockWithItems, EthContractWitnesser};

use ethers::prelude::*;
use num_traits::Zero;

abigen!(KeyManager, "eth-contract-abis/perseverance-rc17/IKeyManager.json");

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

pub struct KeyManager {
	pub deployed_address: H160,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SigData {
	pub sig: ethabi::Uint,
	pub nonce: ethabi::Uint,
	pub k_times_g_address: ethabi::Address,
}

impl Tokenizable for SigData {
	fn from_token(token: ethabi::Token) -> Result<Self, web3::contract::Error>
	where
		Self: Sized,
	{
		if let Token::Tuple(members) = token {
			if members.len() != 3 {
				Err(web3::contract::Error::InvalidOutputType(stringify!(SigData).to_owned()))
			} else {
				Ok(SigData {
					sig: ethabi::Uint::from_token(members[0].clone())?,
					nonce: ethabi::Uint::from_token(members[1].clone())?,
					k_times_g_address: ethabi::Address::from_token(members[2].clone())?,
				})
			}
		} else {
			Err(web3::contract::Error::InvalidOutputType(stringify!(SigData).to_owned()))
		}
	}

	fn into_token(self) -> ethabi::Token {
		Token::Tuple(vec![
			Token::Uint(self.sig),
			Token::Uint(self.nonce),
			Token::Address(self.k_times_g_address),
		])
	}
}

#[async_trait]
impl EthContractWitnesser for KeyManager {
	type EventParameters = KeyManagerEvents;

	fn contract_name(&self) -> String {
		"KeyManager".to_string()
	}

	async fn handle_block_events<StateChainClient, EthRpcClient>(
		&mut self,
		epoch_index: EpochIndex,
		block_number: u64,
		block: BlockWithItems<Event<Self::EventParameters>>,
		state_chain_client: Arc<StateChainClient>,
		eth_rpc: &EthRpcClient,
	) -> anyhow::Result<()>
	where
		EthRpcClient: EthRpcApi + Sync + Send,
		StateChainClient: SignedExtrinsicApi + Send + Sync,
	{
		for event in block.block_items {
			info!("Handling event: {event}");
			match event.event_parameters {
				KeyManagerEvents::AggKeySetByAggKeyFilter(_) => {
					state_chain_client
						.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(
								pallet_cf_vaults::Call::<_, EthereumInstance>::vault_key_rotated {
									block_number,
									tx_id: core_h256(event.tx_hash),
								}
								.into(),
							),
							epoch_index,
						})
						.await;
				},
				KeyManagerEvents::AggKeySetByGovKeyFilter(AggKeySetByGovKeyFilter {
					new_agg_key,
					..
				}) => {
					state_chain_client
						.submit_signed_extrinsic(
							pallet_cf_witnesser::Call::witness_at_epoch {
								call: Box::new(
									pallet_cf_vaults::Call::<_, EthereumInstance>::vault_key_rotated_externally {
										new_public_key:
											cf_chains::eth::AggKey::from_pubkey_compressed(
												new_agg_key.serialize(),
											),
										block_number,
										tx_id: core_h256(event.tx_hash),
									}
									.into(),
								),
								epoch_index,
							},
						)
						.await;
				},
				KeyManagerEvents::SignatureAcceptedFilter(SignatureAcceptedFilter {
					sig_data,
					..
				}) => {
					let TransactionReceipt { gas_used, effective_gas_price, from, .. } =
						eth_rpc.transaction_receipt(event.tx_hash).await?;
					let gas_used = gas_used.context("TransactionReceipt should have gas_used. This might be due to using a light client.")?.try_into().expect("Gas used should fit u128");
					let effective_gas_price = effective_gas_price
						.context("TransactionReceipt should have effective gas price")?
						.try_into()
						.expect("Effective gas price should fit u128");
					state_chain_client
						.submit_signed_extrinsic(
							pallet_cf_witnesser::Call::witness_at_epoch {
								call: Box::new(
									pallet_cf_broadcast::Call::<_, EthereumInstance>::transaction_succeeded {
										tx_out_id: SchnorrVerificationComponents {
											s: sig_data.sig.into(),
											k_times_g_address: sig_data.k_times_g_address.into(),
										},
										signer_id: core_h160(from).into(),
										tx_fee: TransactionFee { effective_gas_price, gas_used },
									}
									.into(),
								),
								epoch_index,
							},
						)
						.await;
				},
				KeyManagerEvents::GovernanceActionFilter(GovernanceActionFilter { message }) => {
					state_chain_client
						.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(
								pallet_cf_governance::Call::set_whitelisted_call_hash {
									call_hash: message,
								}
								.into(),
							),
							epoch_index,
						})
						.await;
				},
				_ => {
					trace!("Ignoring unused event: {event}");
				},
			}
		}

		Ok(())
	}

	fn contract_address(&self) -> H160 {
		self.deployed_address
	}
}

impl KeyManager {
	pub fn new(deployed_address: H160) -> Self {
		Self { deployed_address }
	}
}
