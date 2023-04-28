use crate::state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi;
use std::sync::Arc;

use crate::eth::{EthRpcApi, SignatureAndEvent};

use cf_primitives::EpochIndex;
use sp_runtime::AccountId32;

use tracing::{info, trace};
use web3::{
	ethabi::{self, RawLog},
	types::{H160, H256},
};

use std::fmt::Debug;

use async_trait::async_trait;

use anyhow::{anyhow, bail, Result};

use super::{
	event::Event, utils::decode_log_param, BlockWithItems, DecodeLogClosure, EthContractWitnesser,
	EventParseError,
};

pub struct StateChainGateway {
	pub deployed_address: H160,
	contract: ethabi::Contract,
}

// The following events need to reflect the events emitted in the SC Gateway contract:
// https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/StateChainGateway.sol
#[derive(Debug)]
pub enum StateChainGatewayEvent {
	Funded {
		account_id: AccountId32,
		amount: u128,
		funder: ethabi::Address,
		return_addr: ethabi::Address,
	},

	RedemptionRegistered {
		account_id: AccountId32,
		amount: ethabi::Uint,
		// Withdrawal address
		funder: ethabi::Address,
		start_time: ethabi::Uint,
		expiry_time: ethabi::Uint,
	},

	RedemptionExecuted {
		account_id: AccountId32,
		amount: u128,
	},

	RedemptionExpired {
		account_id: AccountId32,
		amount: u128,
	},

	MinFundingChanged {
		old_min_funding: ethabi::Uint,
		new_min_funding: ethabi::Uint,
	},

	GovernanceWithdrawal {
		to: ethabi::Address,
		amount: u128,
	},

	CommunityGuardDisabled {
		community_guard_disabled: bool,
	},

	FLIPSet {
		flip: ethabi::Address,
	},

	Suspended {
		suspended: bool,
	},

	UpdatedKeyManager {
		key_manager: ethabi::Address,
	},
}

#[async_trait]
impl EthContractWitnesser for StateChainGateway {
	type EventParameters = StateChainGatewayEvent;

	fn contract_name(&self) -> String {
		"StateChainGateway".to_string()
	}

	async fn handle_block_events<StateChainClient, EthRpcClient>(
		&mut self,
		epoch: EpochIndex,
		block_number: u64,
		block: BlockWithItems<Event<Self::EventParameters>>,
		state_chain_client: Arc<StateChainClient>,
		_eth_rpc: &EthRpcClient,
	) -> anyhow::Result<()>
	where
		EthRpcClient: EthRpcApi + Sync + Send,
		StateChainClient: SignedExtrinsicApi + Send + Sync,
	{
		for event in block.block_items {
			info!("Handling event: {event}");
			match event.event_parameters {
				StateChainGatewayEvent::Funded { account_id, amount, funder: _, return_addr } => {
					state_chain_client
						.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(
								pallet_cf_funding::Call::funded {
									account_id,
									amount,
									withdrawal_address: return_addr.0,
									tx_hash: event.tx_hash.into(),
								}
								.into(),
							),
							epoch_index: epoch,
						})
						.await;
				},
				StateChainGatewayEvent::RedemptionExecuted { account_id, amount } => {
					state_chain_client
						.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(
								pallet_cf_funding::Call::redeemed {
									account_id,
									redeemed_amount: amount,
									tx_hash: event.tx_hash.to_fixed_bytes(),
								}
								.into(),
							),
							epoch_index: epoch,
						})
						.await;
				},
				StateChainGatewayEvent::RedemptionExpired { account_id, amount: _ } => {
					state_chain_client
						.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(
								pallet_cf_funding::Call::redemption_expired {
									account_id,
									block_number,
								}
								.into(),
							),
							epoch_index: epoch,
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

	fn decode_log_closure(&self) -> Result<DecodeLogClosure<Self::EventParameters>> {
		let funded = SignatureAndEvent::new(&self.contract, "Funded")?;
		let redemption_registered = SignatureAndEvent::new(&self.contract, "RedemptionRegistered")?;
		let redemption_executed = SignatureAndEvent::new(&self.contract, "RedemptionExecuted")?;
		let redemption_expired = SignatureAndEvent::new(&self.contract, "RedemptionExpired")?;
		let min_funding_changed = SignatureAndEvent::new(&self.contract, "MinFundingChanged")?;
		let gov_withdrawal = SignatureAndEvent::new(&self.contract, "GovernanceWithdrawal")?;
		let community_guard_disabled =
			SignatureAndEvent::new(&self.contract, "CommunityGuardDisabled")?;
		let flip_set = SignatureAndEvent::new(&self.contract, "FLIPSet")?;
		let suspended = SignatureAndEvent::new(&self.contract, "Suspended")?;
		let updated_key_manager = SignatureAndEvent::new(&self.contract, "UpdatedKeyManager")?;

		Ok(Box::new(
			move |event_signature: H256, raw_log: RawLog| -> Result<Self::EventParameters> {
				// get the node_id from the log and return as AccountId32
				let node_id_from_log = |log| {
					let account_bytes: [u8; 32] =
						decode_log_param::<ethabi::FixedBytes>(log, "nodeID")?.try_into().map_err(
							|_| anyhow!("Could not cast FixedBytes nodeID into [u8;32]"),
						)?;
					Result::<_, anyhow::Error>::Ok(AccountId32::new(account_bytes))
				};

				Ok(if event_signature == funded.signature {
					let log = funded.event.parse_log(raw_log)?;
					let account_id = node_id_from_log(&log)?;
					StateChainGatewayEvent::Funded {
						account_id,
						amount: decode_log_param::<ethabi::Uint>(&log, "amount")?
							.try_into()
							.expect("Funded event amount should fit u128"),
						funder: decode_log_param(&log, "funder")?,
						return_addr: decode_log_param(&log, "returnAddr")?,
					}
				} else if event_signature == redemption_registered.signature {
					let log = redemption_registered.event.parse_log(raw_log)?;
					let account_id = node_id_from_log(&log)?;
					StateChainGatewayEvent::RedemptionRegistered {
						account_id,
						amount: decode_log_param(&log, "amount")?,
						funder: decode_log_param(&log, "funder")?,
						start_time: decode_log_param(&log, "startTime")?,
						expiry_time: decode_log_param(&log, "expiryTime")?,
					}
				} else if event_signature == redemption_executed.signature {
					let log = redemption_executed.event.parse_log(raw_log)?;
					let account_id = node_id_from_log(&log)?;
					StateChainGatewayEvent::RedemptionExecuted {
						account_id,
						amount: decode_log_param::<ethabi::Uint>(&log, "amount")?
							.try_into()
							.expect("RedemptionExecuted event amount should fit u128"),
					}
				} else if event_signature == redemption_expired.signature {
					let log = redemption_expired.event.parse_log(raw_log)?;
					let account_id = node_id_from_log(&log)?;
					StateChainGatewayEvent::RedemptionExpired {
						account_id,
						amount: decode_log_param::<ethabi::Uint>(&log, "amount")?
							.try_into()
							.expect("RedemptionExpired event amount should fit u128"),
					}
				} else if event_signature == min_funding_changed.signature {
					let log = min_funding_changed.event.parse_log(raw_log)?;
					StateChainGatewayEvent::MinFundingChanged {
						old_min_funding: decode_log_param(&log, "oldMinFunding")?,
						new_min_funding: decode_log_param(&log, "newMinFunding")?,
					}
				} else if event_signature == gov_withdrawal.signature {
					let log = gov_withdrawal.event.parse_log(raw_log)?;
					StateChainGatewayEvent::GovernanceWithdrawal {
						to: decode_log_param(&log, "to")?,
						amount: decode_log_param::<ethabi::Uint>(&log, "amount")?
							.try_into()
							.expect("GovernanceWithdrawal event amount should fit u128"),
					}
				} else if event_signature == community_guard_disabled.signature {
					let log = community_guard_disabled.event.parse_log(raw_log)?;
					StateChainGatewayEvent::CommunityGuardDisabled {
						community_guard_disabled: decode_log_param(&log, "communityGuardDisabled")?,
					}
				} else if event_signature == flip_set.signature {
					let log = flip_set.event.parse_log(raw_log)?;
					StateChainGatewayEvent::FLIPSet { flip: decode_log_param(&log, "flip")? }
				} else if event_signature == suspended.signature {
					let log = suspended.event.parse_log(raw_log)?;
					StateChainGatewayEvent::Suspended {
						suspended: decode_log_param(&log, "suspended")?,
					}
				} else if event_signature == updated_key_manager.signature {
					let log = updated_key_manager.event.parse_log(raw_log)?;
					StateChainGatewayEvent::UpdatedKeyManager {
						key_manager: decode_log_param(&log, "keyManager")?,
					}
				} else {
					bail!(EventParseError::UnexpectedEvent(event_signature))
				})
			},
		))
	}
}

impl StateChainGateway {
	/// Loads the contract abi to get the event definitions
	pub fn new(deployed_address: H160) -> Self {
		Self {
			deployed_address,
			contract: ethabi::Contract::load(
				std::include_bytes!("abis/StateChainGateway.json").as_ref(),
			)
			.unwrap(),
		}
	}
}

#[cfg(test)]
mod tests {

	use super::*;
	use hex;
	use lazy_static::lazy_static;
	use std::str::FromStr;
	use web3::types::{H256, U256};

	lazy_static! {
		static ref ALICE: H160 =
			web3::types::H160::from_str("0x70997970c51812dc3a010c7d01b50e0d17dc79c8").unwrap();
		static ref NODE_ID: H256 =
			H256::from_str("0x000000000000000000000000000000000000000000000000000000000000a455")
				.unwrap();
	}

	#[test]
	fn test_load_contract() {
		let address = H160::default();
		StateChainGateway::new(address);
	}

	// Convenience test for getting the event signatures for easier searching manually for events
	// with a get_logs query
	#[test]
	#[ignore = "for manual use only"]
	fn generate_signatures() {
		let contract = StateChainGateway::new(H160::default()).contract;

		let funded = SignatureAndEvent::new(&contract, "Funded").unwrap();
		println!("funded {:?}", funded.signature);

		let redemption_registered =
			SignatureAndEvent::new(&contract, "RedemptionRegistered").unwrap();
		println!("redemption_registered {:?}", redemption_registered.signature);

		let redemption_executed = SignatureAndEvent::new(&contract, "RedemptionExecuted").unwrap();
		println!("redemption_executed {:?}", redemption_executed.signature);

		let redemption_expired = SignatureAndEvent::new(&contract, "RedemptionExpired").unwrap();
		println!("redemption_expired {:?}", redemption_expired.signature);

		let min_funding_changed = SignatureAndEvent::new(&contract, "MinFundingChanged").unwrap();
		println!("min_funding_changed {:?}", min_funding_changed.signature);

		let gov_withdrawal = SignatureAndEvent::new(&contract, "GovernanceWithdrawal").unwrap();
		println!("gov_withdrawal {:?}", gov_withdrawal.signature);

		let community_guard_disabled =
			SignatureAndEvent::new(&contract, "CommunityGuardDisabled").unwrap();
		println!("community_guard_disabled {:?}", community_guard_disabled.signature);

		let flip_set = SignatureAndEvent::new(&contract, "FLIPSet").unwrap();
		println!("flip_set {:?}", flip_set.signature);

		let suspended = SignatureAndEvent::new(&contract, "Suspended").unwrap();
		println!("suspended {:?}", suspended.signature);

		let updated_key_manager = SignatureAndEvent::new(&contract, "UpdatedKeyManager").unwrap();
		println!("updated_key_manager {:?}", updated_key_manager.signature);
	}

	#[test]
	fn test_funded_log_parsing() {
		let state_chain_gateway = StateChainGateway::new(H160::default());
		let decode_log = state_chain_gateway.decode_log_closure().unwrap();

		let funded_event_signature =
			H256::from_str("0x0c6eb3554617d242c4c475df7b3342571760bbf3d87ec76852e6f0943a7db896")
				.unwrap();
		match decode_log(
            funded_event_signature,
            RawLog {
                topics : vec![
                    funded_event_signature,
                    *NODE_ID,
                    H256::from_str("0x0000000000000000000000000000000000000000000000000000000000000001").unwrap()
                ],
                data : hex::decode("000000000000000000000000000000000000000000000878678326eac900000000000000000000000000000070997970c51812dc3a010c7d01b50e0d17dc79c8").unwrap()
            }
        ).unwrap() {
            StateChainGatewayEvent::Funded {
                account_id,
                amount,
                funder,
                return_addr,
            } => {
                assert_eq!(account_id, AccountId32::from_str("000000000000000000000000000000000000000000000000000000000000a455").unwrap());
                assert_eq!(amount, 40000000000000000000000u128);
                assert_eq!(funder,ALICE.clone());
                assert_eq!(
                    return_addr,
                    web3::types::H160::from_str("0x0000000000000000000000000000000000000001")
                        .unwrap()
                );
            }
            _ => panic!("Expected StateChainGatewayEvent::Funded, got a different variant"),
        }
	}

	#[test]
	fn test_redemption_registered_log_parsing() {
		let state_chain_gateway = StateChainGateway::new(H160::default());
		let decode_log = state_chain_gateway.decode_log_closure().unwrap();

		let redeemed_register_event_signature =
			H256::from_str("0x2f73775f2573d45f5b0ed0064eb65f631ac9e568a52807221c44ca9d358a9cee")
				.unwrap();
		match decode_log(
            redeemed_register_event_signature,
            RawLog {
                topics : vec![
                    redeemed_register_event_signature,
                    *NODE_ID,
                    H256::from_str("0x00000000000000000000000070997970c51812dc3a010c7d01b50e0d17dc79c8").unwrap()
                ],
                data : hex::decode("0000000000000000000000000000000000000000000002d2cd2bb7a3986000000000000000000000000000000000000000000000000000000000000061a6fd4e0000000000000000000000000000000000000000000000000000000061a9a04b").unwrap()
            }
        ).unwrap() {
            StateChainGatewayEvent::RedemptionRegistered {
                account_id,
                amount,
                funder,
                start_time,
                expiry_time,
            } => {
                assert_eq!(
                    account_id,
                    AccountId32::from_str("000000000000000000000000000000000000000000000000000000000000a455")
                        .unwrap()
                );
                assert_eq!(
                    amount,
                    web3::types::U256::from_dec_str("13333333333333334032384").unwrap()
                );
                assert_eq!(
                    funder, ALICE.clone());
                assert_eq!(
                    start_time,
                    web3::types::U256::from_dec_str("1638333774").unwrap()
                );
                assert_eq!(
                    expiry_time,
                    web3::types::U256::from_dec_str("1638506571").unwrap()
                );
            }
            _ => panic!("Expected Funding::RedemptionRegistered, got a different variant"),
        }
	}

	#[test]
	fn test_redemption_executed_log_parsing() {
		let state_chain_gateway = StateChainGateway::new(H160::default());
		let decode_log = state_chain_gateway.decode_log_closure().unwrap();

		let redeemed_executed_event_signature =
			H256::from_str("0xac96f597a44ad425c6eedf6e4c8327fd959c9d912fa8d027fb54313e59f247c8")
				.unwrap();
		match decode_log(
			redeemed_executed_event_signature,
			RawLog {
				topics: vec![
					redeemed_executed_event_signature,
					H256::from_str(
						"0x000000000000000000000000000000000000000000000000000000000000a455",
					)
					.unwrap(),
				],
				data: hex::decode(
					"0000000000000000000000000000000000000000000002d2cd2bb7a398600000",
				)
				.unwrap(),
			},
		)
		.unwrap()
		{
			StateChainGatewayEvent::RedemptionExecuted { account_id, amount } => {
				assert_eq!(
					account_id,
					AccountId32::from_str(
						"000000000000000000000000000000000000000000000000000000000000a455",
					)
					.unwrap()
				);
				assert_eq!(amount, 13333333333333334032384);
			},
			_ => panic!("Expected Funding::RedemptionExecuted, got a different variant"),
		}
	}

	#[test]
	fn redemption_expired_log_parsing() {
		let state_chain_gateway = StateChainGateway::new(H160::default());
		let decode_log = state_chain_gateway.decode_log_closure().unwrap();

		let redemption_expired_event_signature =
			H256::from_str("0x663304ace90be3e42354c18d4edfd7bf69b1868a8bdba7b9e58de9a997d57714")
				.unwrap();

		const ACCOUNT_ID_HEX: &str =
			"0x000000000000000000000000000000000000000000000000000000000000a455";
		match decode_log(
			redemption_expired_event_signature,
			RawLog {
				topics: vec![
					redemption_expired_event_signature,
					H256::from_str(ACCOUNT_ID_HEX).unwrap(),
				],
				data: hex::decode(
					"00000000000000000000000000000000000000000000001211ede4974a350000",
				)
				.unwrap(),
			},
		)
		.unwrap()
		{
			StateChainGatewayEvent::RedemptionExpired { account_id, amount } => {
				assert_eq!(account_id, AccountId32::from_str(ACCOUNT_ID_HEX).unwrap());
				assert_eq!(amount, 333333333333333311488u128);
			},
			_ => panic!("Expected Funding::RedemptionExpired, got a different variant"),
		}
	}

	#[test]
	fn min_funding_changed_log_parsing() {
		let state_chain_gateway = StateChainGateway::new(H160::default());
		let decode_log = state_chain_gateway.decode_log_closure().unwrap();

		let min_funding_changed_event_signature =
			H256::from_str("0xca11c8a4c461b60c9f485404c272650c2aaae260b2067d72e9924abb68556593")
				.unwrap();
		match decode_log(
            min_funding_changed_event_signature,
            RawLog {
                topics : vec![min_funding_changed_event_signature],
                data : hex::decode("000000000000000000000000000000000000000000000878678326eac90000000000000000000000000000000000000000000000000002d2cd2bb7a398600000").unwrap()
            }
        ).unwrap() {
            StateChainGatewayEvent::MinFundingChanged {
                old_min_funding,
                new_min_funding,
            } => {
                assert_eq!(
                    old_min_funding,
                    U256::from_dec_str("40000000000000000000000").unwrap()
                );
                assert_eq!(
                    new_min_funding,
                    U256::from_dec_str("13333333333333334032384").unwrap()
                );
            }
            _ => panic!("Expected Funding::MinFundingChanged, got a different variant"),
        }
	}

	#[test]
	fn gov_withdrawal_log_parsing() {
		let state_chain_gateway = StateChainGateway::new(H160::default());
		let decode_log = state_chain_gateway.decode_log_closure().unwrap();

		let event_signature =
			H256::from_str("0xfb698a1f0614fe8250cab73f9e958d9eb3aa668918f243f3638dba6da247643d")
				.unwrap();

		match decode_log(
            event_signature,
            RawLog {
                topics: vec![event_signature],
                data: hex::decode(
                    "000000000000000000000000f39fd6e51aad88f6f4ce6ab8827279cfffb9226600000000000000000000000000000000000000000008802b375f23cae2e00000",
                )
                .unwrap(),
            },
        )
        .unwrap()
        {
            StateChainGatewayEvent::GovernanceWithdrawal {
                to,
                amount,
            } => {
                assert_eq!(
                    to,
                    H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266").unwrap()
                );
                assert_eq!(
                    amount,
                    10276666666666666665967616
                );
            }
            _ => panic!("Expected Funding::GovernanceWithdrawal, got a different variant"),
        }
	}

	#[test]
	fn community_guard_disabled_log_parsing() {
		let state_chain_gateway = StateChainGateway::new(H160::default());
		let decode_log = state_chain_gateway.decode_log_closure().unwrap();

		let event_signature =
			H256::from_str("0x057c4cb09f128960151f04372028154acda40272f16360154961672989b59bad")
				.unwrap();

		match decode_log(
			event_signature,
			RawLog {
				topics: vec![event_signature],
				data: hex::decode(
					"0000000000000000000000000000000000000000000000000000000000000001",
				)
				.unwrap(),
			},
		)
		.unwrap()
		{
			StateChainGatewayEvent::CommunityGuardDisabled { community_guard_disabled } => {
				// it is now disabled, so this should be true
				assert!(community_guard_disabled)
			},
			_ => panic!("Expected Funding::CommunityGuardDisabled, got a different variant"),
		};
	}

	#[test]
	fn flip_set_log_parsing() {
		let state_chain_gateway = StateChainGateway::new(H160::default());
		let decode_log = state_chain_gateway.decode_log_closure().unwrap();

		let event_signature =
			H256::from_str("0x28a7be5ead6163acf2999fbd7effa68e097435d695eae192ae3121c9b4e50255")
				.unwrap();

		match decode_log(
			event_signature,
			RawLog {
				topics: vec![event_signature],
				data: hex::decode(
					"000000000000000000000000cf7ed3acca5a467e9e704c703e8d87f634fb0fc9",
				)
				.unwrap(),
			},
		)
		.unwrap()
		{
			StateChainGatewayEvent::FLIPSet { flip } => {
				assert_eq!(
					flip,
					H160::from_str("0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9").unwrap()
				);
			},
			_ => panic!("Expected Funding::FLIPSet, got a different variant"),
		};
	}

	#[test]
	fn suspended_log_parsing() {
		let state_chain_gateway = StateChainGateway::new(H160::default());
		let decode_log = state_chain_gateway.decode_log_closure().unwrap();

		let event_signature =
			H256::from_str("0x58e6c20b68c19f4d8dbc6206267af40b288342464b433205bb41e5b65c4016da")
				.unwrap();

		match decode_log(
			event_signature,
			RawLog {
				topics: vec![event_signature],
				data: hex::decode(
					"0000000000000000000000000000000000000000000000000000000000000001",
				)
				.unwrap(),
			},
		)
		.unwrap()
		{
			StateChainGatewayEvent::Suspended { suspended } => {
				// we are now suspended, so this should be true
				assert!(suspended)
			},
			_ => panic!("Expected Funding::Suspended, got a different variant"),
		};
	}

	#[test]
	fn updated_key_manager_log_parsing() {
		let state_chain_gateway = StateChainGateway::new(H160::default());
		let decode_log = state_chain_gateway.decode_log_closure().unwrap();

		let event_signature =
			H256::from_str("0xd18040e514983d65f088430e69091aea9bf07feaed3696a3faac1ccc34b5e3bc")
				.unwrap();

		match decode_log(
			event_signature,
			RawLog {
				topics: vec![event_signature],
				data: hex::decode(
					"0000000000000000000000000000000000000000000000000000000000000001",
				)
				.unwrap(),
			},
		)
		.unwrap()
		{
			StateChainGatewayEvent::UpdatedKeyManager { key_manager } => {
				assert_eq!(
					key_manager,
					H160::from_str("0x0000000000000000000000000000000000000001").unwrap()
				)
			},
			_ => panic!("Expected Funding::UpdatedKeyManager, got a different variant"),
		};
	}
}
