use crate::{
	eth::{core_h160, core_h256, utils, EthRpcApi, EventParseError, SignatureAndEvent},
	state_chain_observer::client::extrinsic_api::ExtrinsicApi,
};
use cf_chains::eth::{SchnorrVerificationComponents, TransactionFee};
use cf_primitives::EpochIndex;
use state_chain_runtime::EthereumInstance;
use std::sync::Arc;
use web3::{
	contract::tokens::Tokenizable,
	ethabi::{self, RawLog, Token},
	types::{TransactionReceipt, H160, H256},
};

use anyhow::{anyhow, Context, Result};
use pallet_cf_governance::GovCallHash;

use std::fmt::Debug;

use async_trait::async_trait;

use super::{event::Event, BlockWithItems, DecodeLogClosure, EthContractWitnesser};

pub struct KeyManager {
	pub deployed_address: H160,
	pub contract: ethabi::Contract,
}

#[derive(Debug, PartialEq, Eq, Default)]
pub struct ChainflipKey {
	pub_key_x: ethabi::Uint,
	pub_key_y_parity: ethabi::Uint,
}

impl ChainflipKey {
	pub fn from_dec_str(dec_str: &str, parity: bool) -> Result<Self> {
		let pub_key_x = web3::types::U256::from_dec_str(dec_str)?;
		Ok(ChainflipKey {
			pub_key_x,
			pub_key_y_parity: match parity {
				true => web3::types::U256::from(1),
				false => web3::types::U256::from(0),
			},
		})
	}

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

impl Tokenizable for ChainflipKey {
	fn from_token(token: ethabi::Token) -> Result<Self, web3::contract::Error>
	where
		Self: Sized,
	{
		if let Token::Tuple(members) = token {
			if members.len() != 2 {
				Err(web3::contract::Error::InvalidOutputType(stringify!(ChainflipKey).to_owned()))
			} else {
				Ok(ChainflipKey {
					pub_key_x: ethabi::Uint::from_token(members[0].clone())?,
					pub_key_y_parity: ethabi::Uint::from_token(members[1].clone())?,
				})
			}
		} else {
			Err(web3::contract::Error::InvalidOutputType(stringify!(ChainflipKey).to_owned()))
		}
	}

	fn into_token(self) -> ethabi::Token {
		Token::Tuple(vec![
			// Key
			Token::Uint(self.pub_key_x),
			Token::Uint(self.pub_key_y_parity),
		])
	}
}

#[derive(Debug, PartialEq, Eq)]
pub struct SigData {
	pub key_man_addr: ethabi::Address,
	pub chain_id: ethabi::Uint,
	pub msg_hash: ethabi::Uint,
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
			if members.len() != 6 {
				Err(web3::contract::Error::InvalidOutputType(stringify!(SigData).to_owned()))
			} else {
				Ok(SigData {
					key_man_addr: ethabi::Address::from_token(members[0].clone())?,
					chain_id: ethabi::Uint::from_token(members[1].clone())?,
					msg_hash: ethabi::Uint::from_token(members[2].clone())?,
					sig: ethabi::Uint::from_token(members[3].clone())?,
					nonce: ethabi::Uint::from_token(members[4].clone())?,
					k_times_g_address: ethabi::Address::from_token(members[5].clone())?,
				})
			}
		} else {
			Err(web3::contract::Error::InvalidOutputType(stringify!(SigData).to_owned()))
		}
	}

	fn into_token(self) -> ethabi::Token {
		Token::Tuple(vec![
			// Key
			Token::Address(self.key_man_addr),
			Token::Uint(self.chain_id),
			Token::Uint(self.msg_hash),
			Token::Uint(self.sig),
			Token::Uint(self.nonce),
			Token::Address(self.k_times_g_address),
		])
	}
}

// The following events need to reflect the events emitted in the key contract:
// https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/KeyManager.sol
#[derive(Debug, PartialEq, Eq)]
pub enum KeyManagerEvent {
	AggKeyNonceConsumersSet {
		addrs: Vec<ethabi::Address>,
	},

	AggKeyNonceConsumersUpdated {
		new_addrs: Vec<ethabi::Address>,
	},

	AggKeySetByAggKey {
		old_agg_key: ChainflipKey,
		new_agg_key: ChainflipKey,
	},

	AggKeySetByGovKey {
		old_agg_key: ChainflipKey,
		new_agg_key: ChainflipKey,
	},

	CommKeySetByAggKey {
		old_comm_key: ethabi::Address,
		new_comm_key: ethabi::Address,
	},

	CommKeySetByCommKey {
		old_comm_key: ethabi::Address,
		new_comm_key: ethabi::Address,
	},

	GovKeySetByAggKey {
		old_gov_key: ethabi::Address,
		new_gov_key: ethabi::Address,
	},

	GovKeySetByGovKey {
		old_gov_key: ethabi::Address,
		new_gov_key: ethabi::Address,
	},

	SignatureAccepted {
		sig_data: SigData,
		signer: ethabi::Address,
	},

	GovernanceAction {
		/// Call hash of substrate call to be executed, hash over (call, nonce, runtime_version)
		message: GovCallHash,
	},
}

#[async_trait]
impl EthContractWitnesser for KeyManager {
	type EventParameters = KeyManagerEvent;

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
		logger: &slog::Logger,
	) -> anyhow::Result<()>
	where
		EthRpcClient: EthRpcApi + Sync + Send,
		StateChainClient: ExtrinsicApi + Send + Sync,
	{
		for event in block.block_items {
			slog::info!(logger, "Handling event: {}", event);
			match event.event_parameters {
				KeyManagerEvent::AggKeySetByAggKey { new_agg_key, .. } => {
					let _result = state_chain_client
						.submit_signed_extrinsic(
							pallet_cf_witnesser::Call::witness_at_epoch {
								call: Box::new(
									pallet_cf_vaults::Call::<_, EthereumInstance>::vault_key_rotated {
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
							logger,
						)
						.await;
				},
				KeyManagerEvent::AggKeySetByGovKey { new_agg_key, .. } => {
					let _result = state_chain_client
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
							logger,
						)
						.await;
				},
				KeyManagerEvent::SignatureAccepted { sig_data, .. } => {
					let TransactionReceipt { gas_used, effective_gas_price, from, .. } =
						eth_rpc.transaction_receipt(event.tx_hash).await?;
					let gas_used = gas_used.context("TransactionReceipt should have gas_used. This might be due to using a light client.")?.try_into().expect("Gas used should fit u128");
					let effective_gas_price = effective_gas_price
						.context("TransactionReceipt should have effective gas price")?
						.try_into()
						.expect("Effective gas price should fit u128");
					let _result = state_chain_client
						.submit_signed_extrinsic(
							pallet_cf_witnesser::Call::witness_at_epoch {
								call: Box::new(
									pallet_cf_broadcast::Call::<_, EthereumInstance>::signature_accepted {
										signature: SchnorrVerificationComponents {
											s: sig_data.sig.into(),
											k_times_g_address: sig_data.k_times_g_address.into(),
										},
										signer_id: core_h160(from),
										tx_fee: TransactionFee { effective_gas_price, gas_used },
									}
									.into(),
								),
								epoch_index,
							},
							logger,
						)
						.await;
				},
				KeyManagerEvent::GovernanceAction { message } => {
					let _result = state_chain_client
						.submit_signed_extrinsic(
							pallet_cf_witnesser::Call::witness_at_epoch {
								call: Box::new(
									pallet_cf_governance::Call::set_whitelisted_call_hash {
										call_hash: message,
									}
									.into(),
								),
								epoch_index,
							},
							logger,
						)
						.await;
				},
				_ => {
					slog::trace!(logger, "Ignoring unused event: {}", event);
				},
			}
		}

		Ok(())
	}

	fn decode_log_closure(&self) -> Result<DecodeLogClosure<Self::EventParameters>> {
		let ak_nonce_consumers_set =
			SignatureAndEvent::new(&self.contract, "AggKeyNonceConsumersSet")?;

		let ak_nonce_consumers_updated =
			SignatureAndEvent::new(&self.contract, "AggKeyNonceConsumersUpdated")?;

		let ak_set_by_ak = SignatureAndEvent::new(&self.contract, "AggKeySetByAggKey")?;
		let ak_set_by_gk = SignatureAndEvent::new(&self.contract, "AggKeySetByGovKey")?;

		let ck_set_by_ak = SignatureAndEvent::new(&self.contract, "CommKeySetByAggKey")?;
		let ck_set_by_ck = SignatureAndEvent::new(&self.contract, "CommKeySetByCommKey")?;

		let gk_set_by_ak = SignatureAndEvent::new(&self.contract, "GovKeySetByAggKey")?;
		let gk_set_by_gk = SignatureAndEvent::new(&self.contract, "GovKeySetByGovKey")?;

		let gov_action = SignatureAndEvent::new(&self.contract, "GovernanceAction")?;
		let sig_accepted = SignatureAndEvent::new(&self.contract, "SignatureAccepted")?;

		Ok(Box::new(move |event_signature: H256, raw_log: RawLog| -> Result<KeyManagerEvent> {
			Ok(if event_signature == ak_nonce_consumers_set.signature {
				let log = ak_nonce_consumers_set.event.parse_log(raw_log)?;
				KeyManagerEvent::AggKeyNonceConsumersSet {
					addrs: utils::decode_log_param(&log, "addrs")?,
				}
			} else if event_signature == ak_nonce_consumers_updated.signature {
				let log = ak_nonce_consumers_updated.event.parse_log(raw_log)?;
				KeyManagerEvent::AggKeyNonceConsumersUpdated {
					new_addrs: utils::decode_log_param(&log, "newAddrs")?,
				}
			} else if event_signature == ak_set_by_ak.signature {
				let log = ak_set_by_ak.event.parse_log(raw_log)?;
				KeyManagerEvent::AggKeySetByAggKey {
					old_agg_key: utils::decode_log_param::<ChainflipKey>(&log, "oldAggKey")?,
					new_agg_key: utils::decode_log_param::<ChainflipKey>(&log, "newAggKey")?,
				}
			} else if event_signature == ak_set_by_gk.signature {
				let log = ak_set_by_gk.event.parse_log(raw_log)?;
				KeyManagerEvent::AggKeySetByGovKey {
					old_agg_key: utils::decode_log_param::<ChainflipKey>(&log, "oldAggKey")?,
					new_agg_key: utils::decode_log_param::<ChainflipKey>(&log, "newAggKey")?,
				}
			} else if event_signature == ck_set_by_ak.signature {
				let log = ck_set_by_ak.event.parse_log(raw_log)?;
				KeyManagerEvent::CommKeySetByAggKey {
					old_comm_key: utils::decode_log_param::<ethabi::Address>(&log, "oldCommKey")?,
					new_comm_key: utils::decode_log_param::<ethabi::Address>(&log, "newCommKey")?,
				}
			} else if event_signature == ck_set_by_ck.signature {
				let log = ck_set_by_ck.event.parse_log(raw_log)?;
				KeyManagerEvent::CommKeySetByCommKey {
					old_comm_key: utils::decode_log_param::<ethabi::Address>(&log, "oldCommKey")?,
					new_comm_key: utils::decode_log_param::<ethabi::Address>(&log, "newCommKey")?,
				}
			} else if event_signature == gk_set_by_ak.signature {
				let log = gk_set_by_ak.event.parse_log(raw_log)?;
				KeyManagerEvent::GovKeySetByAggKey {
					old_gov_key: utils::decode_log_param(&log, "oldGovKey")?,
					new_gov_key: utils::decode_log_param(&log, "newGovKey")?,
				}
			} else if event_signature == gk_set_by_gk.signature {
				let log = gk_set_by_gk.event.parse_log(raw_log)?;
				KeyManagerEvent::GovKeySetByGovKey {
					old_gov_key: utils::decode_log_param(&log, "oldGovKey")?,
					new_gov_key: utils::decode_log_param(&log, "newGovKey")?,
				}
			} else if event_signature == sig_accepted.signature {
				let log = sig_accepted.event.parse_log(raw_log)?;
				KeyManagerEvent::SignatureAccepted {
					sig_data: utils::decode_log_param::<SigData>(&log, "sigData")?,
					signer: utils::decode_log_param(&log, "signer")?,
				}
			} else if event_signature == gov_action.signature {
				let log = gov_action.event.parse_log(raw_log)?;
				KeyManagerEvent::GovernanceAction {
					message: utils::decode_log_param(&log, "message")?,
				}
			} else {
				return Err(anyhow!(EventParseError::UnexpectedEvent(event_signature)))
			})
		}))
	}

	fn contract_address(&self) -> H160 {
		self.deployed_address
	}
}

impl KeyManager {
	/// Loads the contract abi to get the event definitions
	pub fn new(deployed_address: H160) -> Self {
		Self {
			deployed_address,
			contract: ethabi::Contract::load(std::include_bytes!("abis/KeyManager.json").as_ref())
				.unwrap(),
		}
	}
}

#[cfg(test)]
mod tests {

	use crate::eth::EventParseError;

	use super::*;
	use hex;
	use std::str::FromStr;
	use web3::types::{H256, U256};

	// All log data for these tests was obtained from the events in the `deploy_and` script:
	// https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/scripts/deploy_and.py

	// All the key strings in this test are decimal pub keys derived from the priv keys in the
	// consts.py script https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/tests/consts.py

	// Convenience test to allow us to generate the signatures of the events, allowing us
	// to manually query the contract for the events
	// current signatures below:
	// ak_nonce_consumers_set: 0x4d44910489c7d151e8e9e918a73a0081a95b08fd2d8f2011a6e99548d2f585eb
	// ak_nonce_consumers_updated:
	// 0x4f2c4ca40026b3ddbe8c1f23b9dc777d3ebaa9f1a30baa8de230d6b556b1a04f ak_set_by_ak:
	// 0x5cba64f32f2576e404f74394dc04611cce7416e299c94db0667d4e315e852521 ak_set_by_gk:
	// 0xe441a6cf7a12870075eb2f6399c0de122bfe6cd8a75bfa83b05d5b611552532e ck_set_by_ak:
	// 0x999bc9c97358a1254b8ba2c1e65893b34385bf27c448cb21af3f19eee6b809ce ck_set_by_ck:
	// 0xb8529adc43e07de6ef9ce6a65ca2e5ad5f52b155e85bbbc28f7d3c165170deab gk_set_by_ak:
	// 0x6049e088bb150ffb9041c7bfd3f7d4017d79a930d2d23e2f331eeffb0cb74297 gk_set_by_gk:
	// 0xb79780665df55038fba66988b1b3f2eda919a59b75cd2581f31f8f04f58bec7c gov_action:
	// 0x06e69d4af70b00b0c269b2707345abc134d9767085930456d9d03285f1eaf5c7 sig_accepted:
	// 0x38045dba3d9ee1fee641ad521bd1cf34c28562f6658772ee04678edf17b9a3bc
	#[test]
	fn generate_signatures() {
		let contract = KeyManager::new(H160::default()).contract;

		let ak_nonce_consumers_set =
			SignatureAndEvent::new(&contract, "AggKeyNonceConsumersSet").unwrap();
		println!("ak_nonce_consumers_set: {:?}", ak_nonce_consumers_set.signature);
		let ak_nonce_consumers_updated =
			SignatureAndEvent::new(&contract, "AggKeyNonceConsumersUpdated").unwrap();
		println!("ak_nonce_consumers_updated: {:?}", ak_nonce_consumers_updated.signature);
		let ak_set_by_ak = SignatureAndEvent::new(&contract, "AggKeySetByAggKey").unwrap();
		println!("ak_set_by_ak: {:?}", ak_set_by_ak.signature);
		let ak_set_by_gk = SignatureAndEvent::new(&contract, "AggKeySetByGovKey").unwrap();
		println!("ak_set_by_gk: {:?}", ak_set_by_gk.signature);
		let ck_set_by_ak = SignatureAndEvent::new(&contract, "CommKeySetByAggKey").unwrap();
		println!("ck_set_by_ak: {:?}", ck_set_by_ak.signature);
		let ck_set_by_ck = SignatureAndEvent::new(&contract, "CommKeySetByCommKey").unwrap();
		println!("ck_set_by_ck: {:?}", ck_set_by_ck.signature);
		let gk_set_by_ak = SignatureAndEvent::new(&contract, "GovKeySetByAggKey").unwrap();
		println!("gk_set_by_ak: {:?}", gk_set_by_ak.signature);
		let gk_set_by_gk = SignatureAndEvent::new(&contract, "GovKeySetByGovKey").unwrap();
		println!("gk_set_by_gk: {:?}", gk_set_by_gk.signature);
		let gov_action = SignatureAndEvent::new(&contract, "GovernanceAction").unwrap();
		println!("gov_action: {:?}", gov_action.signature);
		let sig_accepted = SignatureAndEvent::new(&contract, "SignatureAccepted").unwrap();
		println!("sig_accepted: {:?}", sig_accepted.signature);
	}

	fn new_test_key_manager() -> KeyManager {
		KeyManager::new(H160::default())
	}

	#[test]
	fn test_ak_nonce_consumers_set() {
		let key_manager = new_test_key_manager();
		let decode_log = key_manager.decode_log_closure().unwrap();
		let event_signature =
			H256::from_str("4d44910489c7d151e8e9e918a73a0081a95b08fd2d8f2011a6e99548d2f585eb")
				.unwrap();

		match decode_log(
            event_signature,
            RawLog {
                topics: vec![event_signature],
                data: hex::decode(
                    "00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000e7f1725e7734ce288f8367e1bb143e90bb3f05120000000000000000000000009fe46736679d2d9a65f0992f2272de9f3c7fa6e0000000000000000000000000cf7ed3acca5a467e9e704c703e8d87f634fb0fc9",
                )
                .unwrap(),
            },
        )
        .expect("Failed parsing KeyManagerEvent::AggKeyNonceConsumerSet event")
        {
            KeyManagerEvent::AggKeyNonceConsumersSet { addrs } => {
                assert_eq!(addrs, vec![H160::from_str("0xe7f1725e7734ce288f8367e1bb143e90bb3f0512").unwrap(), H160::from_str("0x9fe46736679d2d9a65f0992f2272de9f3c7fa6e0").unwrap(), H160::from_str("0xcf7ed3acca5a467e9e704c703e8d87f634fb0fc9").unwrap()]);
            }
            _ => panic!("Expected KeyManagerEvent::AggKeyNonceConsumerSet, got different variant"),
        }
	}

	#[test]
	fn test_ak_nonce_consumers_updated() {
		let key_manager = new_test_key_manager();
		let decode_log = key_manager.decode_log_closure().unwrap();
		let event_signature =
			H256::from_str("4f2c4ca40026b3ddbe8c1f23b9dc777d3ebaa9f1a30baa8de230d6b556b1a04f")
				.unwrap();

		match decode_log(
            event_signature,
            RawLog {
                topics: vec![event_signature],
                data: hex::decode(
                    "000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000003000000000000000000000000e7f1725e7734ce288f8367e1bb143e90bb3f05120000000000000000000000009fe46736679d2d9a65f0992f2272de9f3c7fa6e0000000000000000000000000cf7ed3acca5a467e9e704c703e8d87f634fb0fc90000000000000000000000000000000000000000000000000000000000000003000000000000000000000000e7f1725e7734ce288f8367e1bb143e90bb3f05120000000000000000000000009fe46736679d2d9a65f0992f2272de9f3c7fa6e0000000000000000000000000cf7ed3acca5a467e9e704c703e8d87f634fb0fc9",
                )
                .unwrap(),
            },
        )
        .expect("Failed parsing KeyManagerEvent::AggKeyNonceConsumersUpdated event")
        {
            KeyManagerEvent::AggKeyNonceConsumersUpdated { new_addrs } => {
                assert_eq!(new_addrs, vec![H160::from_str("0xe7f1725e7734ce288f8367e1bb143e90bb3f0512").unwrap(), H160::from_str("0x9fe46736679d2d9a65f0992f2272de9f3c7fa6e0").unwrap(), H160::from_str("0xcf7ed3acca5a467e9e704c703e8d87f634fb0fc9").unwrap()]);
            }
            _ => panic!("Expected KeyManagerEvent::AggKeyNonceConsumersUpdated, got different variant"),
        }
	}

	// 🔑 Aggregate Key sets the new Aggregate Key 🔑
	#[test]
	fn test_ak_set_by_ak_parsing() {
		let key_manager = new_test_key_manager();
		let decode_log = key_manager.decode_log_closure().unwrap();
		let event_signature =
			H256::from_str("0x5cba64f32f2576e404f74394dc04611cce7416e299c94db0667d4e315e852521")
				.unwrap();

		match decode_log(
                event_signature,
                RawLog {
                    topics : vec![event_signature],
                    data : hex::decode("31b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae00000000000000000000000000000000000000000000000000000000000000011742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d0000000000000000000000000000000000000000000000000000000000000001").unwrap()
                }
            ).expect("Failed parsing KeyManagerEvent::AggKeySetByAggKey event") {
                KeyManagerEvent::AggKeySetByAggKey {
                    old_agg_key,
                    new_agg_key,
                } => {
                    assert_eq!(old_agg_key, ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());
                    assert_eq!(new_agg_key, ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap());
                }
                _ => panic!("Expected KeyManagerEvent::AggKeySetByAggKey, got different variant"),
            }
	}

	// 🔑 Governance Key sets the new Aggregate Key 🔑
	#[test]
	fn test_ak_set_gk_parsing() {
		let key_manager = new_test_key_manager();
		let decode_log = key_manager.decode_log_closure().unwrap();
		let event_signature =
			H256::from_str("0xe441a6cf7a12870075eb2f6399c0de122bfe6cd8a75bfa83b05d5b611552532e")
				.unwrap();

		match decode_log(
                event_signature,
                RawLog {
                    topics : vec![event_signature],
                    data : hex::decode("1742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d000000000000000000000000000000000000000000000000000000000000000131b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae0000000000000000000000000000000000000000000000000000000000000001").unwrap()
                }
            ).expect("Failed parsing KeyManagerEvent::AggKeySetByGovKey event")
            {
                KeyManagerEvent::AggKeySetByGovKey {
                    old_agg_key,
                    new_agg_key,
                } => {
                    assert_eq!(old_agg_key, ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap());
                    assert_eq!(new_agg_key, ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());
                }
                _ => panic!("Expected KeyManagerEvent::AggKeySetByGovKey, got different variant"),
            }
	}

	// 🔑 Governance Key sets the new Governance Key 🔑
	#[test]
	fn test_gk_set_by_gk_parsing() {
		let key_manager = new_test_key_manager();
		let decode_log = key_manager.decode_log_closure().unwrap();
		let event_signature =
			H256::from_str("0xb79780665df55038fba66988b1b3f2eda919a59b75cd2581f31f8f04f58bec7c")
				.unwrap();

		match decode_log(
                event_signature,
                RawLog {
                    topics : vec![event_signature],
                    data : hex::decode("000000000000000000000000f39fd6e51aad88f6f4ce6ab8827279cfffb922660000000000000000000000009965507d1a55bcc2695c58ba16fb37d819b0a4dc").unwrap()
                }
            ).expect("Failed parsing KeyManagerEvent::GovKeySetByGovKey event")
            {
                KeyManagerEvent::GovKeySetByGovKey {
                    old_gov_key,
                    new_gov_key,
                } => {
                    assert_eq!(old_gov_key, H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266").unwrap());
                    assert_eq!(new_gov_key, H160::from_str("0x9965507d1a55bcc2695c58ba16fb37d819b0a4dc").unwrap());
                }
                _ => panic!("Expected KeyManagerEvent::GovKeySetByGovKey, got different variant"),
            }
	}

	// 🔑 Aggregate Key sets the new Governance Key 🔑
	#[test]
	fn test_gk_set_by_ak_parsing() {
		let key_manager = new_test_key_manager();
		let decode_log = key_manager.decode_log_closure().unwrap();
		let event_signature =
			H256::from_str("0x6049e088bb150ffb9041c7bfd3f7d4017d79a930d2d23e2f331eeffb0cb74297")
				.unwrap();

		match decode_log(
                    event_signature,
                    RawLog {
                        topics : vec![event_signature],
                        data : hex::decode("0000000000000000000000009965507d1a55bcc2695c58ba16fb37d819b0a4dc000000000000000000000000f39fd6e51aad88f6f4ce6ab8827279cfffb92266").unwrap()
                    }
                ).expect("Failed parsing KeyManagerEvent::GovKeySetByAggKey event")
                {
                    KeyManagerEvent::GovKeySetByAggKey {
                        old_gov_key,
                        new_gov_key,
                    } => {
                        assert_eq!(old_gov_key, H160::from_str("0x9965507d1a55bcc2695c58ba16fb37d819b0a4dc").unwrap());
                        assert_eq!(new_gov_key, H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266").unwrap());
                    }
                    _ => panic!("Expected KeyManagerEvent::GovKeySetByAggKey, got different variant"),
                }
	}

	// 🔑 Comm Key sets the new Comm Key 🔑
	#[test]
	fn test_ck_set_by_ck_parsing() {
		let key_manager = new_test_key_manager();
		let decode_log = key_manager.decode_log_closure().unwrap();
		let event_signature =
			H256::from_str("0xb8529adc43e07de6ef9ce6a65ca2e5ad5f52b155e85bbbc28f7d3c165170deab")
				.unwrap();

		match decode_log(
                        event_signature,
                        RawLog {
                            topics : vec![event_signature],
                            data : hex::decode("000000000000000000000000976ea74026e726554db657fa54763abd0c3a0aa900000000000000000000000014dc79964da2c08b23698b3d3cc7ca32193d9955").unwrap()
                        }
                    ).expect("Failed parsing KeyManagerEvent::CommKeySetByCommKey event")
                    {
                        KeyManagerEvent::CommKeySetByCommKey {
                            old_comm_key,
                            new_comm_key,
                        } => {
                            assert_eq!(old_comm_key, H160::from_str("0x976ea74026e726554db657fa54763abd0c3a0aa9").unwrap());
                            assert_eq!(new_comm_key, H160::from_str("0x14dc79964da2c08b23698b3d3cc7ca32193d9955").unwrap());
                        }
                        _ => panic!("Expected KeyManagerEvent::CommKeySetByCommKey, got different variant"),
                    }
	}

	// 🔑 Comm Key sets the new Comm Key 🔑
	#[test]
	fn test_ck_set_by_agg_key_parsing() {
		let key_manager = new_test_key_manager();
		let decode_log = key_manager.decode_log_closure().unwrap();
		let event_signature =
			H256::from_str("0x999bc9c97358a1254b8ba2c1e65893b34385bf27c448cb21af3f19eee6b809ce")
				.unwrap();

		match decode_log(
                            event_signature,
                            RawLog {
                                topics : vec![event_signature],
                                data : hex::decode("00000000000000000000000014dc79964da2c08b23698b3d3cc7ca32193d9955000000000000000000000000976ea74026e726554db657fa54763abd0c3a0aa9").unwrap()
                            }
                        ).expect("Failed parsing KeyManagerEvent::CommKeySetByAggKey event")
                        {
                            KeyManagerEvent::CommKeySetByAggKey {
                                old_comm_key,
                                new_comm_key,
                            } => {
                                assert_eq!(old_comm_key, H160::from_str("0x14dc79964da2c08b23698b3d3cc7ca32193d9955").unwrap());
                                assert_eq!(new_comm_key, H160::from_str("0x976ea74026e726554db657fa54763abd0c3a0aa9").unwrap());
                            }
                            _ => panic!("Expected KeyManagerEvent::CommKeySetByAggKey, got different variant"),
                        }
	}

	// Governance Action
	#[test]
	fn test_gov_action_parsing() {
		let key_manager = new_test_key_manager();
		let decode_log = key_manager.decode_log_closure().unwrap();
		let event_signature =
			H256::from_str("0x06e69d4af70b00b0c269b2707345abc134d9767085930456d9d03285f1eaf5c7")
				.unwrap();

		match decode_log(
			event_signature,
			RawLog {
				topics: vec![event_signature],
				data: hex::decode(
					"000000000000000000000000000000000000000000000000000000000000a455",
				)
				.unwrap(),
			},
		)
		.expect("Failed parsing KeyManagerEvent::GovernanceAction event")
		{
			KeyManagerEvent::GovernanceAction { message } => {
				assert_eq!(message, H256::from_low_u64_be(42069).as_ref());
			},
			_ => panic!("Expected KeyManagerEvent::GovernanceAction, got different variant"),
		}
	}

	#[test]
	fn test_sig_accepted_parsing() {
		let key_manager = new_test_key_manager();
		let decode_log = key_manager.decode_log_closure().unwrap();
		let event_signature =
			H256::from_str("0x38045dba3d9ee1fee641ad521bd1cf34c28562f6658772ee04678edf17b9a3bc")
				.unwrap();

		match decode_log(
            event_signature,
            RawLog {
                topics: vec![event_signature],
                data: hex::decode(
                    "000000000000000000000000e7f1725e7734ce288f8367e1bb143e90bb3f05120000000000000000000000000000000000000000000000000000000000007a69b918a2687d109fa0308fedb39f0dd091accd9edb80a9ddb2ccb1f0abaa6cfb64ed5ecfedaacc9bd0bcc5512e7fcf9650de5619acc0a747681f58d26f66468e7000000000000000000000000000000000000000000000000000000000000000030000000000000000000000007ceb2425ec324348ba69bd50205b11e29770fd96000000000000000000000000f39fd6e51aad88f6f4ce6ab8827279cfffb92266",
                )
                .unwrap(),
            },
        )
        .expect("Failed parsing KeyManagerEvent::SignatureAccepted event")
        {
            KeyManagerEvent::SignatureAccepted {
                sig_data,
                signer,
            } => {
                assert_eq!(sig_data, SigData{
                    key_man_addr: H160::from_str("0xe7f1725e7734ce288f8367e1bb143e90bb3f0512").unwrap(),
                    chain_id: U256::from_dec_str("31337").unwrap(),
                    msg_hash: U256::from_dec_str("83721402217372471513450062042778477963861354613529233808466400078111064259428").unwrap(),
                    sig: U256::from_dec_str("107365663807311708634605056423336732647043554150507905924516852373709157469808").unwrap(),
                    nonce: U256::from_dec_str("3").unwrap(),
                    k_times_g_address: H160::from_str("0x7ceb2425ec324348ba69bd50205b11e29770fd96").unwrap(),
                });
                assert_eq!(signer, H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266").unwrap());
            }
            _ => panic!("Expected KeyManagerEvent::SignatureAccepted, got different variant"),
        }
	}

	#[test]
	fn test_invalid_sig() {
		let key_manager = new_test_key_manager();
		let decode_log = key_manager.decode_log_closure().unwrap();
		let invalid_signature =
			H256::from_str("0x0b0b5ed18390ab49777844d5fcafb9865c74095ceb3e73cc57d1fbcc926103b5")
				.unwrap();

		let res = decode_log(
                invalid_signature,
                RawLog {
                    topics : vec![invalid_signature],
                    data : hex::decode("000000000000000000000000000000000000000000000000000000000000000131b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae00000000000000000000000000000000000000000000000000000000000000011742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d0000000000000000000000000000000000000000000000000000000000000001").unwrap()
                }
            )
            .map_err(|e| match e.downcast_ref::<EventParseError>() {
                Some(EventParseError::UnexpectedEvent(_)) => {}
                _ => {
                    panic!("Incorrect error parsing INVALID_SIG_LOG");
                }
            });
		assert!(res.is_err());
	}

	#[test]
	fn test_chainflip_key_serialize() {
		use secp256k1::PublicKey;

		// Create a `ChainflipKey` and a `PublicKey` that are the same
		let cf_key = ChainflipKey::from_dec_str(
			"22479114112312168431982914496826057754130808976066989807481484372215659188398",
			true,
		)
		.unwrap();

		let sk = secp256k1::SecretKey::from_str(
			"fbcb47bc85b881e0dfb31c872d4e06848f80530ccbd18fc016a27c4a744d0eba",
		)
		.unwrap();

		let secp_key = PublicKey::from_secret_key(&secp256k1::Secp256k1::signing_only(), &sk);

		// Compare the serialize() values to make sure we serialize the same as secp256k1
		assert_eq!(cf_key.serialize(), secp_key.serialize());
	}
}
