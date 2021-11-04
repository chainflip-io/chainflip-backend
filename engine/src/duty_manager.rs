use crate::p2p::AccountId;
//use crate::state_chain::sc_event;
use cf_chains::ChainId;
use pallet_cf_vaults::BlockHeight;
use slog::o;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

//use substrate_subxt::{Client, EventSubscription};

use cf_traits::{ChainflipAccountData, ChainflipAccountState, EpochIndex};
//use sp_core::storage::StorageChangeSet;

use crate::logging::COMPONENT_KEY;
// use crate::state_chain::pallets::validator::NewEpochEvent;
// use crate::state_chain::runtime::StateChainRuntime;

pub enum NodeState {
    // Only monitoring for storage change events
    Passive,
    // The node must be liave
    // Only submits heartbeats + monitors storage change events
    BackupValidator,

    // For now we'll put
    // Only submits heartbeats, but was active in the previous epoch
    Outgoing,

    /// THIS MUST TAKE PRECEDENCE, IT COULD CAPTURE OUTGOING TO
    // Is on the Active Validator list
    // Uses the active_windows to filter witnessing.
    RunningValidator,
}

pub struct DutyManager {
    account_id: AccountId,
    node_state: NodeState,
    /// The epoch that the chain is currently in
    current_epoch: EpochIndex,
    _account_state: ChainflipAccountState,
    /// Contains the block at which we start our validator duties for each respective chain
    start_duties_at: HashMap<ChainId, BlockHeight>,
}

impl DutyManager {
    // Called after we have the current epoch.
    pub fn new(account_id: AccountId) -> DutyManager {
        DutyManager {
            account_id,
            node_state: NodeState::Passive,
            _account_state: ChainflipAccountState::Passive,
            start_duties_at: HashMap::new(),
            current_epoch: 0,
        }
    }

    /// Creates a Duty Manager in RunningValidator state with 0's account ID
    #[cfg(test)]
    pub fn new_test() -> DutyManager {
        let test_account_id: [u8; 32] = [0; 32];
        DutyManager {
            account_id: AccountId(test_account_id),
            node_state: NodeState::RunningValidator,
            _account_state: ChainflipAccountState::Passive,
            start_duties_at: HashMap::new(),
            current_epoch: 0,
        }
    }

    /// Check if the heartbeat is enabled
    pub fn is_heartbeat_enabled(&self) -> bool {
        match self.node_state {
            NodeState::BackupValidator | NodeState::RunningValidator => true,
            NodeState::Passive => false,
            NodeState::Outgoing => todo!(),
        }
    }

    /// check if the sc observer is observing all events or just the KeygenRequestEvent
    pub fn is_sc_observation_enabled(&self) -> bool {
        match self.node_state {
            NodeState::RunningValidator => true,
            NodeState::BackupValidator | NodeState::Passive => false,
            NodeState::Outgoing => todo!(),
        }
    }

    /// check if our account id is in the provided list
    pub fn account_list_contains_account_id(&self, account_list: &Vec<AccountId>) -> bool {
        account_list.contains(&self.account_id)
    }

    // THIS METHOD DOES NOT WORK.
    // /// Check if we were an active validator at a specified block number and increments total_witnessed_events if true
    pub fn is_active_validator_for_chain_at(&self, chain_id: ChainId, block_number: u64) -> bool {
        match self.node_state {
            NodeState::RunningValidator => todo!(),
            _ => false,
        }
        // match self.node_state {
        //     NodeState::RunningValidator => {
        //         match self.start_duties.iter().find(|(a, _)| a == &chain_id) {
        //             Some((_, active_window))
        //                 if active_validator_at(active_window, block_number) =>
        //             {
        //                 true
        //             }
        //             _ => false,
        //         }
        //     }
        //     _ => false,
        // }
    }

    pub fn set_current_epoch(&mut self, epoch_index: EpochIndex) {
        self.current_epoch = epoch_index;
    }

    // fn process_storage_change(&mut self, set: StorageChangeSet) {
    //     // use the StorageChangeSet to change the state and active windows
    //     for change in set.iter() {
    //         match change {
    //             // Some(key) => {

    //             // }
    //             _ => {}
    //         }
    //     }
    // }
}

use crate::state_chain::client::{StateChainClient, StateChainRpcApi};
use futures::{Stream, StreamExt};

// This should be on its own task?
pub async fn start_duty_manager<BlockStream, RpcClient>(
    duty_manager: Arc<RwLock<DutyManager>>,
    state_chain_client: Arc<StateChainClient<RpcClient>>,
    sc_block_stream: BlockStream,
    logger: &slog::Logger,
) where
    BlockStream: Stream<Item = anyhow::Result<state_chain_runtime::Header>>,
    RpcClient: StateChainRpcApi,
{
    let logger = logger.new(o!(COMPONENT_KEY => "DutyManager"));
    slog::info!(logger, "Starting");

    // let current_epoch = state_chain_client.cur

    // Get our node state from the block stream
    let mut sc_block_stream = Box::pin(sc_block_stream);
    while let Some(result_block_header) = sc_block_stream.next().await {
        match result_block_header {
            Ok(block_header) => {
                // TODO: Optimise this so it's not run every block

                // we have our account data
                let my_state_for_this_block =
                    state_chain_client.node_status(&block_header).await.unwrap();

                // let if

                match my_state_for_this_block {
                    ChainflipAccountState::Validator => {
                        let mut dm = duty_manager.write().await;
                        dm.node_state = NodeState::RunningValidator;
                    }
                    ChainflipAccountState::Backup => {
                        let mut dm = duty_manager.write().await;
                        dm.node_state = NodeState::BackupValidator;
                    }
                    _ => {
                        let mut dm = duty_manager.write().await;
                        dm.node_state = NodeState::Passive;
                    }
                }
            }
            Err(error) => {
                slog::error!(logger, "Failed to decode block header: {}", error,);
            }
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use crate::state_chain::client::connect_to_state_chain;
//     use crate::{logging, settings};
//     use std::sync::Arc;

//     use super::*;

//     #[tokio::test]
//     #[ignore = "depends on sc"]
//     async fn debug() {
//         let settings = settings::test_utils::new_test_settings().unwrap();
//         let logger = logging::test_utils::new_test_logger();

//         let (state_chain_client, block_stream) = connect_to_state_chain(&settings).await.unwrap();

//         let duty_manager = Arc::new(RwLock::new(DutyManager::new_test()));

//         let duty_manager_fut = start_duty_manager(
//             duty_manager.clone(),
//             state_chain_client,
//             block_stream,
//             &logger,
//         );

//         tokio::join!(duty_manager_fut,);
//     }

//     #[test]
//     fn test_active_validator_window() {
//         let active_window = BlockHeightWindow {
//             from: 10,
//             to: Some(20),
//         };
//         assert!(active_validator_at(&active_window, 15));
//         assert!(active_validator_at(&active_window, 20));
//         assert!(active_validator_at(&active_window, 10));
//         assert!(!active_validator_at(&active_window, 1));
//         assert!(!active_validator_at(&active_window, 21));
//         let active_window = BlockHeightWindow {
//             from: 100,
//             to: None,
//         };
//         assert!(!active_validator_at(&active_window, 50));
//         assert!(active_validator_at(&active_window, 150));
//     }

//     #[tokio::test]
//     async fn test_is_active_validator_at() {
//         let duty_manager = Arc::new(RwLock::new(DutyManager::new_test()));

//         let mut dm = duty_manager.write().await;
//         assert!(!dm.is_active_validator_at(ChainId::Ethereum, 0));
//         dm.active_windows
//             .push((ChainId::Ethereum, BlockHeightWindow { from: 0, to: None }));
//         assert!(dm.is_active_validator_at(ChainId::Ethereum, 0));
//         assert!(dm.is_active_validator_at(ChainId::Ethereum, 100000));

//         dm.active_windows.clear();
//         dm.active_windows.push((
//             ChainId::Ethereum,
//             BlockHeightWindow {
//                 from: 10,
//                 to: Some(20),
//             },
//         ));
//         assert!(!dm.is_active_validator_at(ChainId::Ethereum, 9));
//         assert!(dm.is_active_validator_at(ChainId::Ethereum, 10));
//         assert!(dm.is_active_validator_at(ChainId::Ethereum, 20));
//         assert!(!dm.is_active_validator_at(ChainId::Ethereum, 21));
//     }
// }
