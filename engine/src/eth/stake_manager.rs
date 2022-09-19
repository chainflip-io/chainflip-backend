use crate::state_chain_observer::client::StateChainClient;
use std::sync::Arc;

use crate::{
    eth::{utils, EthRpcApi, SignatureAndEvent},
    state_chain_observer::client::StateChainRpcApi,
};

use cf_primitives::EpochIndex;
use sp_runtime::AccountId32;

use web3::{
    ethabi::{self, RawLog},
    types::{H160, H256},
};

use std::fmt::Debug;

use async_trait::async_trait;

use anyhow::{anyhow, Result};

use super::{
    contract_witnesser::ContractStateUpdate, event::Event, DecodeLogClosure, EthContractWitnesser,
    EventParseError,
};

pub struct StakeManager {
    pub deployed_address: H160,
    contract: ethabi::Contract,
}

// The following events need to reflect the events emitted in the staking contract:
// https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/StakeManager.sol
#[derive(Debug)]
pub enum StakeManagerEvent {
    Staked {
        account_id: AccountId32,
        amount: u128,
        staker: ethabi::Address,
        return_addr: ethabi::Address,
    },

    ClaimRegistered {
        account_id: AccountId32,
        amount: ethabi::Uint,
        // Withdrawal address
        staker: ethabi::Address,
        start_time: ethabi::Uint,
        expiry_time: ethabi::Uint,
    },

    ClaimExecuted {
        account_id: AccountId32,
        amount: u128,
    },

    MinStakeChanged {
        old_min_stake: ethabi::Uint,
        new_min_stake: ethabi::Uint,
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

pub struct StakeManagerContractState {}

impl ContractStateUpdate for StakeManagerContractState {
    type Item = ();
    type Event = StakeManagerEvent;
}

#[async_trait]
impl EthContractWitnesser for StakeManager {
    type EventParameters = StakeManagerEvent;
    type StateItem = ();

    fn contract_name(&self) -> &'static str {
        "StakeManager"
    }

    async fn handle_event<RpcClient, EthRpcClient, ContractWitnesserState>(
        &self,
        epoch: EpochIndex,
        _block_number: u64,
        event: Event<Self::EventParameters>,
        _filter_state: &ContractWitnesserState,
        state_chain_client: Arc<StateChainClient<RpcClient>>,
        _eth_rpc: &EthRpcClient,
        logger: &slog::Logger,
    ) -> anyhow::Result<()>
    where
        RpcClient: 'static + StateChainRpcApi + Sync + Send,
        EthRpcClient: EthRpcApi + Sync + Send,
        ContractWitnesserState: Send
            + Sync
            + ContractStateUpdate<Event = Self::EventParameters, Item = Self::StateItem>,
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
                        pallet_cf_witnesser::Call::witness_at_epoch {
                            call: Box::new(
                                pallet_cf_staking::Call::staked {
                                    account_id,
                                    amount,
                                    withdrawal_address: return_addr.0,
                                    tx_hash: event.tx_hash.into(),
                                }
                                .into(),
                            ),
                            epoch_index: epoch,
                        },
                        logger,
                    )
                    .await;
            }
            StakeManagerEvent::ClaimExecuted { account_id, amount } => {
                let _result = state_chain_client
                    .submit_signed_extrinsic(
                        pallet_cf_witnesser::Call::witness_at_epoch {
                            call: Box::new(
                                pallet_cf_staking::Call::claimed {
                                    account_id,
                                    claimed_amount: amount,
                                    tx_hash: event.tx_hash.to_fixed_bytes(),
                                }
                                .into(),
                            ),
                            epoch_index: epoch,
                        },
                        logger,
                    )
                    .await;
            }
            _ => {
                slog::trace!(logger, "Ignoring unused event: {}", event);
            }
        }

        Ok(())
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
                        utils::decode_log_param::<ethabi::FixedBytes>(log, "nodeID")?
                            .try_into()
                            .map_err(|_| {
                                anyhow!("Could not cast FixedBytes nodeID into [u8;32]")
                            })?;
                    Result::<_, anyhow::Error>::Ok(AccountId32::new(account_bytes))
                };

                Ok(if event_signature == staked.signature {
                    let log = staked.event.parse_log(raw_log)?;
                    let account_id = node_id_from_log(&log)?;
                    StakeManagerEvent::Staked {
                        account_id,
                        amount: utils::decode_log_param::<ethabi::Uint>(&log, "amount")?.as_u128(),
                        staker: utils::decode_log_param(&log, "staker")?,
                        return_addr: utils::decode_log_param(&log, "returnAddr")?,
                    }
                } else if event_signature == claim_registered.signature {
                    let log = claim_registered.event.parse_log(raw_log)?;
                    let account_id = node_id_from_log(&log)?;
                    StakeManagerEvent::ClaimRegistered {
                        account_id,
                        amount: utils::decode_log_param(&log, "amount")?,
                        staker: utils::decode_log_param(&log, "staker")?,
                        start_time: utils::decode_log_param(&log, "startTime")?,
                        expiry_time: utils::decode_log_param(&log, "expiryTime")?,
                    }
                } else if event_signature == claim_executed.signature {
                    let log = claim_executed.event.parse_log(raw_log)?;
                    let account_id = node_id_from_log(&log)?;
                    StakeManagerEvent::ClaimExecuted {
                        account_id,
                        amount: utils::decode_log_param::<ethabi::Uint>(&log, "amount")?.as_u128(),
                    }
                } else if event_signature == min_stake_changed.signature {
                    let log = min_stake_changed.event.parse_log(raw_log)?;
                    StakeManagerEvent::MinStakeChanged {
                        old_min_stake: utils::decode_log_param(&log, "oldMinStake")?,
                        new_min_stake: utils::decode_log_param(&log, "newMinStake")?,
                    }
                } else if event_signature == gov_withdrawal.signature {
                    let log = gov_withdrawal.event.parse_log(raw_log)?;
                    StakeManagerEvent::GovernanceWithdrawal {
                        to: utils::decode_log_param(&log, "to")?,
                        amount: utils::decode_log_param::<ethabi::Uint>(&log, "amount")?.as_u128(),
                    }
                } else if event_signature == community_guard_disabled.signature {
                    let log = community_guard_disabled.event.parse_log(raw_log)?;
                    StakeManagerEvent::CommunityGuardDisabled {
                        community_guard_disabled: utils::decode_log_param(
                            &log,
                            "communityGuardDisabled",
                        )?,
                    }
                } else if event_signature == flip_set.signature {
                    let log = flip_set.event.parse_log(raw_log)?;
                    StakeManagerEvent::FLIPSet {
                        flip: utils::decode_log_param(&log, "flip")?,
                    }
                } else if event_signature == suspended.signature {
                    let log = suspended.event.parse_log(raw_log)?;
                    StakeManagerEvent::Suspended {
                        suspended: utils::decode_log_param(&log, "suspended")?,
                    }
                } else if event_signature == updated_key_manager.signature {
                    let log = updated_key_manager.event.parse_log(raw_log)?;
                    StakeManagerEvent::UpdatedKeyManager {
                        key_manager: utils::decode_log_param(&log, "keyManager")?,
                    }
                } else {
                    return Err(anyhow!(EventParseError::UnexpectedEvent(event_signature)));
                })
            },
        ))
    }
}

impl StakeManager {
    /// Loads the contract abi to get the event definitions
    pub fn new(deployed_address: H160) -> Self {
        Self {
            deployed_address,
            contract: ethabi::Contract::load(
                std::include_bytes!("abis/StakeManager.json").as_ref(),
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
        StakeManager::new(address);
    }

    #[test]
    fn test_staked_log_parsing() {
        let stake_manager = StakeManager::new(H160::default());
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
        let stake_manager = StakeManager::new(H160::default());
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
        let stake_manager = StakeManager::new(H160::default());
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
        let stake_manager = StakeManager::new(H160::default());
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
        let stake_manager = StakeManager::new(H160::default());
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

    #[test]
    fn community_guard_disabled_log_parsing() {
        let stake_manager = StakeManager::new(H160::default());
        let decode_log = stake_manager.decode_log_closure().unwrap();

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
            StakeManagerEvent::CommunityGuardDisabled {
                community_guard_disabled,
            } => {
                // it is now disabled, so this should be true
                assert!(community_guard_disabled)
            }
            _ => panic!("Expected Staking::CommunityGuardDisabled, got a different variant"),
        };
    }

    #[test]
    fn flip_set_log_parsing() {
        let stake_manager = StakeManager::new(H160::default());
        let decode_log = stake_manager.decode_log_closure().unwrap();

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
            StakeManagerEvent::FLIPSet { flip } => {
                assert_eq!(
                    flip,
                    H160::from_str("0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9").unwrap()
                );
            }
            _ => panic!("Expected Staking::FLIPSet, got a different variant"),
        };
    }

    #[test]
    fn suspended_log_parsing() {
        let stake_manager = StakeManager::new(H160::default());
        let decode_log = stake_manager.decode_log_closure().unwrap();

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
            StakeManagerEvent::Suspended { suspended } => {
                // we are now suspended, so this should be true
                assert!(suspended)
            }
            _ => panic!("Expected Staking::Suspended, got a different variant"),
        };
    }

    #[test]
    fn updated_key_manager_log_parsing() {
        let stake_manager = StakeManager::new(H160::default());
        let decode_log = stake_manager.decode_log_closure().unwrap();

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
            StakeManagerEvent::UpdatedKeyManager { key_manager } => {
                assert_eq!(
                    key_manager,
                    H160::from_str("0x0000000000000000000000000000000000000001").unwrap()
                )
            }
            _ => panic!("Expected Staking::UpdatedKeyManager, got a different variant"),
        };
    }
}
