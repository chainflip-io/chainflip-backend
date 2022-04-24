//! Contains the information required to use the StakeManger contract as a source for
//! the EthEventStreamer

use crate::state_chain::client::StateChainClient;
use std::{convert::TryInto, sync::Arc};

use crate::{
    eth::{utils, SignatureAndEvent},
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

use super::{event_common::EventWithCommon, DecodeLogClosure, EthObserver, EventParseError};

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

    /// `MinStakeChanged(oldMinStake, newMinStake)`
    MinStakeChanged {
        /// Old minimum stake
        old_min_stake: ethabi::Uint,
        /// New minimum stake
        new_min_stake: ethabi::Uint,
    },

    /// `GovernanceWithdrawal(to, amount)`
    GovernanceWithdrawal {
        /// Withdrawal address
        to: ethabi::Address,
        /// Withdrawal amount
        amount: u128,
    },
}

#[async_trait]
impl EthObserver for StakeManager {
    type EventParameters = StakeManagerEvent;

    fn contract_name(&self) -> &'static str {
        "StakeManager"
    }

    async fn handle_event<RpcClient>(
        &self,
        event: EventWithCommon<Self::EventParameters>,
        state_chain_client: Arc<StateChainClient<RpcClient>>,
        logger: &slog::Logger,
    ) where
        RpcClient: 'static + StateChainRpcApi + Sync + Send,
    {
        slog::info!(logger, "Handling event: {}", event);
        match event.event_parameters {
            StakeManagerEvent::Staked {
                account_id,
                amount,
                staker: _,
                return_addr,
            } => {
                let _result = state_chain_client
                    .submit_signed_extrinsic(
                        pallet_cf_witnesser_api::Call::witness_staked(
                            account_id,
                            amount,
                            return_addr.0,
                            event.tx_hash.into(),
                        ),
                        logger,
                    )
                    .await;
            }
            StakeManagerEvent::ClaimExecuted { account_id, amount } => {
                let _result = state_chain_client
                    .submit_signed_extrinsic(
                        pallet_cf_witnesser_api::Call::witness_claimed(
                            account_id,
                            amount,
                            event.tx_hash.to_fixed_bytes(),
                        ),
                        logger,
                    )
                    .await;
            }
            _ => {
                slog::trace!(logger, "Ignoring unused event: {}", event);
            }
        }
    }

    fn get_contract_address(&self) -> H160 {
        self.deployed_address
    }

    fn decode_log_closure(&self) -> Result<DecodeLogClosure<Self::EventParameters>> {
        let staked = SignatureAndEvent::new(&self.contract, "Staked")?;
        let claim_registered = SignatureAndEvent::new(&self.contract, "ClaimRegistered")?;
        let claim_executed = SignatureAndEvent::new(&self.contract, "ClaimExecuted")?;
        let min_stake_changed = SignatureAndEvent::new(&self.contract, "MinStakeChanged")?;
        let gov_withdrawal = SignatureAndEvent::new(&self.contract, "GovernanceWithdrawal")?;

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
                } else if signature == min_stake_changed.signature {
                    let log = min_stake_changed.event.parse_log(raw_log)?;
                    StakeManagerEvent::MinStakeChanged {
                        old_min_stake: utils::decode_log_param(&log, "oldMinStake")?,
                        new_min_stake: utils::decode_log_param(&log, "newMinStake")?,
                    }
                } else if signature == gov_withdrawal.signature {
                    let log = gov_withdrawal.event.parse_log(raw_log)?;
                    StakeManagerEvent::GovernanceWithdrawal {
                        to: utils::decode_log_param(&log, "to")?,
                        amount: utils::decode_log_param::<ethabi::Uint>(&log, "amount")?.as_u128(),
                    }
                } else {
                    return Err(anyhow::anyhow!(EventParseError::UnexpectedEvent(signature)));
                })
            },
        ))
    }
}

impl StakeManager {
    /// Loads the contract abi to get the event definitions
    pub fn new(deployed_address: H160) -> Result<Self> {
        Ok(Self {
            deployed_address,
            contract: ethabi::Contract::load(
                std::include_bytes!("abis/StakeManager.json").as_ref(),
            )?,
        })
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
        assert_ok!(StakeManager::new(address));
    }

    #[test]
    fn test_staked_log_parsing() {
        let stake_manager = StakeManager::new(H160::default()).unwrap();
        let decode_log = stake_manager.decode_log_closure().unwrap();

        let staked_event_signature =
            H256::from_str("0x0c6eb3554617d242c4c475df7b3342571760bbf3d87ec76852e6f0943a7db896")
                .unwrap();
        match decode_log(
            staked_event_signature,
            RawLog {
                topics : vec![
                    staked_event_signature,
                    *NODE_ID,
                    H256::from_str("0x0000000000000000000000000000000000000000000000000000000000000001").unwrap()
                ],
                data : hex::decode("000000000000000000000000000000000000000000000878678326eac900000000000000000000000000000070997970c51812dc3a010c7d01b50e0d17dc79c8").unwrap()
            }
        ).unwrap() {
            StakeManagerEvent::Staked {
                account_id,
                amount,
                staker,
                return_addr,
            } => {
                assert_eq!(account_id, AccountId32::from_str("000000000000000000000000000000000000000000000000000000000000a455").unwrap());
                assert_eq!(amount, 40000000000000000000000u128);
                assert_eq!(staker,ALICE.clone());
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
        let stake_manager = StakeManager::new(H160::default()).unwrap();
        let decode_log = stake_manager.decode_log_closure().unwrap();

        let claimed_register_event_signature =
            H256::from_str("0x2f73775f2573d45f5b0ed0064eb65f631ac9e568a52807221c44ca9d358a9cee")
                .unwrap();
        match decode_log(
            claimed_register_event_signature,
            RawLog {
                topics : vec![
                    claimed_register_event_signature,
                    *NODE_ID,
                    H256::from_str("0x00000000000000000000000070997970c51812dc3a010c7d01b50e0d17dc79c8").unwrap()
                ],
                data : hex::decode("0000000000000000000000000000000000000000000002d2cd2bb7a3986000000000000000000000000000000000000000000000000000000000000061a6fd4e0000000000000000000000000000000000000000000000000000000061a9a04b").unwrap()
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
                    AccountId32::from_str("000000000000000000000000000000000000000000000000000000000000a455")
                        .unwrap()
                );
                assert_eq!(
                    amount,
                    web3::types::U256::from_dec_str("13333333333333334032384").unwrap()
                );
                assert_eq!(
                    staker, ALICE.clone());
                assert_eq!(
                    start_time,
                    web3::types::U256::from_dec_str("1638333774").unwrap()
                );
                assert_eq!(
                    expiry_time,
                    web3::types::U256::from_dec_str("1638506571").unwrap()
                );
            }
            _ => panic!("Expected Staking::ClaimRegistered, got a different variant"),
        }
    }

    #[test]
    fn test_claim_executed_log_parsing() {
        let stake_manager = StakeManager::new(H160::default()).unwrap();
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
            StakeManagerEvent::ClaimExecuted { account_id, amount } => {
                assert_eq!(
                    account_id,
                    AccountId32::from_str(
                        "000000000000000000000000000000000000000000000000000000000000a455",
                    )
                    .unwrap()
                );
                assert_eq!(amount, 13333333333333334032384);
            }
            _ => panic!("Expected Staking::ClaimExecuted, got a different variant"),
        }
    }

    #[test]
    fn min_stake_changed_log_parsing() {
        let stake_manager = StakeManager::new(H160::default()).unwrap();
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
    fn gov_withdrawal_log_parsing() {
        let stake_manager = StakeManager::new(H160::default()).unwrap();
        let decode_log = stake_manager.decode_log_closure().unwrap();

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
            StakeManagerEvent::GovernanceWithdrawal {
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
            _ => panic!("Expected Staking::GovernanceWithdrawal, got a different variant"),
        }
    }
}
