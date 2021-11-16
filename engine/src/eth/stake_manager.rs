//! Contains the information required to use the StakeManger contract as a source for
//! the EthEventStreamer

use crate::state_chain::client::StateChainClient;
use std::{convert::TryInto, sync::Arc};

use crate::{
    eth::{utils, SignatureAndEvent},
    settings,
    state_chain::client::StateChainRpcApi,
};

use sp_runtime::AccountId32;

use web3::{
    ethabi::{self, RawLog},
    types::{H160, H256},
};

use std::fmt::Debug;

use async_trait::async_trait;

use anyhow::Result;

use super::{decode_shared_event_closure, event_common::EventWithCommon, EthObserver, SharedEvent};

/// A wrapper for the StakeManager Ethereum contract.
pub struct StakeManager {
    pub deployed_address: H160,
    contract: ethabi::Contract,
}

/// Represents the events that are expected from the StakeManager contract.
#[derive(Debug)]
pub enum StakeManagerEvent {
    /// The `Staked(nodeId, amount)` event.
    Staked {
        /// The node id of the validator that submitted the stake.
        account_id: AccountId32,
        /// The amount of FLIP that was staked.
        amount: u128,
        /// The address which made the `Stake` transaction
        staker: ethabi::Address,
        /// The address which the staker requires to be used when claiming back FLIP for `nodeID`
        return_addr: ethabi::Address,
    },

    /// `ClaimRegistered(nodeId, amount, staker, startTime, expiryTime)` event
    ClaimRegistered {
        /// Node id of the validator registering the claim
        account_id: AccountId32,
        /// Amount the validator is claiming
        amount: ethabi::Uint,
        /// The ETH address of the validator, used to stake their FLIP
        staker: ethabi::Address,
        /// The start time of the claim
        start_time: ethabi::Uint,
        /// The expiry time of the claim
        expiry_time: ethabi::Uint,
    },

    /// `ClaimExecuted(nodeId, amount)` event
    ClaimExecuted {
        /// The node id of the validator that claimed their FLIP
        account_id: AccountId32,
        /// The amount of FLIP that was claimed
        amount: u128,
    },

    /// `FlipSupplyUpdated(oldSupply, newTotalSupply, stateChainBlockNumber)` event
    FlipSupplyUpdated {
        /// Old emission per block
        old_supply: ethabi::Uint,
        /// New emission per block
        new_supply: ethabi::Uint,
        /// State Chain block number for the new total supply
        block_number: ethabi::Uint,
    },

    /// `MinStakeChanged(oldMinStake, newMinStake)`
    MinStakeChanged {
        /// Old minimum stake
        old_min_stake: ethabi::Uint,
        /// New minimum stake
        new_min_stake: ethabi::Uint,
    },

    // TODO: Should be able to remove shared from here
    /// Events that both the Key and Stake Manager contracts can output (Shared.sol)
    Shared(SharedEvent),
}

#[async_trait]
impl EthObserver for StakeManager {
    type EventParameters = StakeManagerEvent;

    async fn handle_event<RPCClient>(
        &self,
        event: EventWithCommon<Self::EventParameters>,
        state_chain_client: Arc<StateChainClient<RPCClient>>,
        logger: &slog::Logger,
    ) where
        RPCClient: 'static + StateChainRpcApi + Sync + Send,
    {
        match event.event_parameters {
            StakeManagerEvent::Staked {
                account_id,
                amount,
                staker: _,
                return_addr,
            } => {
                let _ = state_chain_client
                    .submit_extrinsic(
                        &logger,
                        pallet_cf_witnesser_api::Call::witness_staked(
                            account_id,
                            amount,
                            return_addr.0,
                            event.tx_hash,
                        ),
                    )
                    .await;
            }
            StakeManagerEvent::ClaimExecuted { account_id, amount } => {
                let _ = state_chain_client
                    .submit_extrinsic(
                        &logger,
                        pallet_cf_witnesser_api::Call::witness_claimed(
                            account_id,
                            amount,
                            event.tx_hash,
                        ),
                    )
                    .await;
            }
            ignored_event => {
                slog::warn!(
                    logger,
                    "{:?} is not to be submitted to the State Chain",
                    ignored_event
                );
            }
        }
    }

    fn get_deployed_address(&self) -> H160 {
        self.deployed_address
    }

    fn decode_log_closure(
        &self,
    ) -> Result<Box<dyn Fn(H256, ethabi::RawLog) -> Result<Self::EventParameters> + Send>> {
        let staked = SignatureAndEvent::new(&self.contract, "Staked")?;
        let claim_registered = SignatureAndEvent::new(&self.contract, "ClaimRegistered")?;
        let claim_executed = SignatureAndEvent::new(&self.contract, "ClaimExecuted")?;
        let flip_supply_updated = SignatureAndEvent::new(&self.contract, "FlipSupplyUpdated")?;
        let min_stake_changed = SignatureAndEvent::new(&self.contract, "MinStakeChanged")?;

        let decode_shared_event_closure = decode_shared_event_closure(&self.contract)?;

        Ok(Box::new(
            move |signature: H256, raw_log: RawLog| -> Result<Self::EventParameters> {
                // get the node_id from the log and return as AccountId32
                let node_id_from_log = |log| {
                    let account_bytes: [u8; 32] =
                        utils::decode_log_param::<ethabi::FixedBytes>(log, "nodeID")?
                            .try_into()
                            .map_err(|_| {
                                anyhow::Error::msg("Could not cast FixedBytes nodeID into [u8;32]")
                            })?;
                    Result::<_, anyhow::Error>::Ok(AccountId32::new(account_bytes))
                };

                Ok(if signature == staked.signature {
                    let log = staked.event.parse_log(raw_log)?;
                    let account_id = node_id_from_log(&log)?;
                    StakeManagerEvent::Staked {
                        account_id,
                        amount: utils::decode_log_param::<ethabi::Uint>(&log, "amount")?.as_u128(),
                        staker: utils::decode_log_param(&log, "staker")?,
                        return_addr: utils::decode_log_param(&log, "returnAddr")?,
                    }
                } else if signature == claim_registered.signature {
                    let log = claim_registered.event.parse_log(raw_log)?;
                    let account_id = node_id_from_log(&log)?;
                    StakeManagerEvent::ClaimRegistered {
                        account_id,
                        amount: utils::decode_log_param(&log, "amount")?,
                        staker: utils::decode_log_param(&log, "staker")?,
                        start_time: utils::decode_log_param(&log, "startTime")?,
                        expiry_time: utils::decode_log_param(&log, "expiryTime")?,
                    }
                } else if signature == claim_executed.signature {
                    let log = claim_executed.event.parse_log(raw_log)?;
                    let account_id = node_id_from_log(&log)?;
                    StakeManagerEvent::ClaimExecuted {
                        account_id,
                        amount: utils::decode_log_param::<ethabi::Uint>(&log, "amount")?.as_u128(),
                    }
                } else if signature == flip_supply_updated.signature {
                    let log = flip_supply_updated.event.parse_log(raw_log)?;
                    StakeManagerEvent::FlipSupplyUpdated {
                        old_supply: utils::decode_log_param(&log, "oldSupply")?,
                        new_supply: utils::decode_log_param(&log, "newSupply")?,
                        block_number: utils::decode_log_param(&log, "stateChainBlockNumber")?,
                    }
                } else if signature == min_stake_changed.signature {
                    let log = min_stake_changed.event.parse_log(raw_log)?;
                    StakeManagerEvent::MinStakeChanged {
                        old_min_stake: utils::decode_log_param(&log, "oldMinStake")?,
                        new_min_stake: utils::decode_log_param(&log, "newMinStake")?,
                    }
                } else {
                    StakeManagerEvent::Shared(decode_shared_event_closure(signature, raw_log)?)
                })
            },
        ))
    }
}

impl StakeManager {
    /// Loads the contract abi to get event definitions
    pub fn new(settings: &settings::Settings) -> Result<Self> {
        let contract =
            ethabi::Contract::load(std::include_bytes!("abis/StakeManager.json").as_ref())?;
        Ok(Self {
            deployed_address: settings.eth.stake_manager_eth_address,
            contract,
        })
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use hex;
    use std::str::FromStr;
    use web3::types::{H256, U256};

    #[test]
    fn test_load_contract() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        assert_ok!(StakeManager::new(&settings));
    }

    #[test]
    fn test_staked_log_parsing() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let stake_manager = StakeManager::new(&settings).unwrap();
        let decode_log = stake_manager.decode_log_closure().unwrap();

        let staked_event_signature =
            H256::from_str("0x0c6eb3554617d242c4c475df7b3342571760bbf3d87ec76852e6f0943a7db896")
                .unwrap();
        match decode_log(
            staked_event_signature,
            RawLog {
                topics : vec![
                    staked_event_signature,
                    H256::from_str("0x0000000000000000000000000000000000000000000000000000000000003039").unwrap()
                ],
                data : hex::decode("000000000000000000000000000000000000000000000878678326eac900000000000000000000000000000070997970c51812dc3a010c7d01b50e0d17dc79c80000000000000000000000000000000000000000000000000000000000000001").unwrap()
            }
        ).unwrap() {
            StakeManagerEvent::Staked {
                account_id,
                amount,
                staker,
                return_addr,
            } => {
                let expected_account_id =
                    AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU")
                        .unwrap();
                assert_eq!(account_id, expected_account_id);
                assert_eq!(amount, 40000000000000000000000u128);
                assert_eq!(staker,
                    web3::types::H160::from_str("0x70997970c51812dc3a010c7d01b50e0d17dc79c8")
                    .unwrap()
                );
                assert_eq!(
                    return_addr,
                    web3::types::H160::from_str("0x0000000000000000000000000000000000000001")
                        .unwrap()
                );
            }
            _ => panic!("Expected StakeManagerEvent::Staked, got a different variant"),
        }
    }

    #[test]
    fn test_claim_registered_log_parsing() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let stake_manager = StakeManager::new(&settings).unwrap();
        let decode_log = stake_manager.decode_log_closure().unwrap();

        let claimed_register_event_signature =
            H256::from_str("0x2f73775f2573d45f5b0ed0064eb65f631ac9e568a52807221c44ca9d358a9cee")
                .unwrap();
        match decode_log(
            claimed_register_event_signature,
            RawLog {
                topics : vec![
                    claimed_register_event_signature,
                    H256::from_str("0x0000000000000000000000000000000000000000000000000000000000003039").unwrap()
                ],
                data : hex::decode("0000000000000000000000000000000000000000000002d2cd2bb7a39860000000000000000000000000000073d669c173d88ccb01f6daab3a3304af7a1b22c10000000000000000000000000000000000000000000000000000000060d4910f0000000000000000000000000000000000000000000000000000000060d73402").unwrap()
            }
        ).unwrap() {
            StakeManagerEvent::ClaimRegistered {
                account_id,
                amount,
                staker,
                start_time,
                expiry_time,
            } => {
                assert_eq!(
                    account_id,
                    AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU")
                        .unwrap()
                );
                assert_eq!(
                    amount,
                    web3::types::U256::from_dec_str("13333333333333334032384").unwrap()
                );
                assert_eq!(
                    staker,
                    web3::types::H160::from_str("0x73d669c173d88ccb01f6daab3a3304af7a1b22c1")
                        .unwrap()
                );
                assert_eq!(
                    start_time,
                    web3::types::U256::from_dec_str("1624543503").unwrap()
                );
                assert_eq!(
                    expiry_time,
                    web3::types::U256::from_dec_str("1624716290").unwrap()
                );
            }
            _ => panic!("Expected Staking::ClaimRegistered, got a different variant"),
        }
    }

    #[test]
    fn test_claim_executed_log_parsing() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let stake_manager = StakeManager::new(&settings).unwrap();
        let decode_log = stake_manager.decode_log_closure().unwrap();

        let claimed_executed_event_signature =
            H256::from_str("0xac96f597a44ad425c6eedf6e4c8327fd959c9d912fa8d027fb54313e59f247c8")
                .unwrap();
        match decode_log(
            claimed_executed_event_signature,
            RawLog {
                topics: vec![
                    claimed_executed_event_signature,
                    H256::from_str(
                        "0x0000000000000000000000000000000000000000000000000000000000003039",
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
            StakeManagerEvent::ClaimExecuted { account_id, amount } => {
                let expected_node_id =
                    AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU")
                        .unwrap();
                assert_eq!(account_id, expected_node_id);
                assert_eq!(amount, 13333333333333334032384);
            }
            _ => panic!("Expected Staking::ClaimExecuted, got a different variant"),
        }
    }

    #[test]
    fn flip_supply_updated_log_parsing() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let stake_manager = StakeManager::new(&settings).unwrap();
        let decode_log = stake_manager.decode_log_closure().unwrap();

        let flip_supply_updated_event_signature =
            H256::from_str("0xff4b7a826623672c6944dc44d809008e2e1105180d110fd63986e841f15eb2ad")
                .unwrap();
        match decode_log(
            flip_supply_updated_event_signature,
            RawLog {
                topics : vec![flip_supply_updated_event_signature],
                data : hex::decode("0000000000000000000000000000000000000000004a723dc6b40b8a9a00000000000000000000000000000000000000000000000052b7d2dcc80cd2e40000000000000000000000000000000000000000000000000000000000000000000064").unwrap()
            }
        ).unwrap() {
            StakeManagerEvent::FlipSupplyUpdated {
                old_supply,
                new_supply,
                block_number,
            } => {
                assert_eq!(
                    old_supply,
                    U256::from_dec_str("90000000000000000000000000").unwrap()
                );
                assert_eq!(
                    new_supply,
                    U256::from_dec_str("100000000000000000000000000").unwrap()
                );
                assert_eq!(block_number, U256::from_dec_str("100").unwrap());
            }
            _ => panic!("Expected Staking::FlipSupplyUpdated, got a different variant"),
        }
    }

    #[test]
    fn min_stake_changed_log_parsing() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let stake_manager = StakeManager::new(&settings).unwrap();
        let decode_log = stake_manager.decode_log_closure().unwrap();

        let min_stake_changed_event_signature =
            H256::from_str("0xca11c8a4c461b60c9f485404c272650c2aaae260b2067d72e9924abb68556593")
                .unwrap();
        match decode_log(
            min_stake_changed_event_signature,
            RawLog {
                topics : vec![min_stake_changed_event_signature],
                data : hex::decode("000000000000000000000000000000000000000000000878678326eac90000000000000000000000000000000000000000000000000002d2cd2bb7a398600000").unwrap()
            }
        ).unwrap() {
            StakeManagerEvent::MinStakeChanged {
                old_min_stake,
                new_min_stake,
            } => {
                assert_eq!(
                    old_min_stake,
                    U256::from_dec_str("40000000000000000000000").unwrap()
                );
                assert_eq!(
                    new_min_stake,
                    U256::from_dec_str("13333333333333334032384").unwrap()
                );
            }
            _ => panic!("Expected Staking::MinStakeChanged, got a different variant"),
        }
    }

    #[test]
    fn refunded_log_parsing() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let stake_manager = StakeManager::new(&settings).unwrap();
        let decode_log = stake_manager.decode_log_closure().unwrap();

        let refunded_event_signature =
            H256::from_str("0x3d2a04f53164bedf9a8a46353305d6b2d2261410406df3b41f99ce6489dc003c")
                .unwrap();

        match decode_log(
            refunded_event_signature,
            RawLog {
                topics: vec![refunded_event_signature],
                data: hex::decode(
                    "00000000000000000000000000000000000000000000000000000a1eaa1e2544",
                )
                .unwrap(),
            },
        )
        .unwrap()
        {
            StakeManagerEvent::Shared(SharedEvent::Refunded { amount }) => {
                assert_eq!(11126819398980, amount);
            }
            _ => panic!("Expected StakeManagerEvent::Refunded, got a different variant"),
        }
    }
}
