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

/// Represents the different "action" states the CFE can be in
/// These only have rough mappings to the State Chain's idea of a node's state
pub enum NodeState {
    // Only monitoring for storage change events - so we know when/if we should transition to another state
    Passive,
    // The node must be liave
    // Only submits heartbeats + monitors storage change events
    // Backup Validators and Outgoing Validators (which may be Backup too) fall into this category
    HeartbeatAndWatch,

    // Is on the Active Validator list
    // Uses the active_windows to filter witnessing.
    // Outgoing, until the last ETH block is done being witnessed - do we want to cache this somewhere? - or use a Last consensus block method
    Running,
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
    pub fn new(account_id: AccountId, current_epoch: EpochIndex) -> DutyManager {
        DutyManager {
            account_id,
            node_state: NodeState::Passive,
            _account_state: ChainflipAccountState::Passive,
            start_duties_at: HashMap::new(),
            current_epoch,
        }
    }

    /// Creates a Duty Manager in RunningValidator state with 0's account ID
    #[cfg(test)]
    pub fn new_test() -> DutyManager {
        let test_account_id: [u8; 32] = [0; 32];
        DutyManager {
            account_id: AccountId(test_account_id),
            node_state: NodeState::Running,
            _account_state: ChainflipAccountState::Passive,
            start_duties_at: HashMap::new(),
            current_epoch: 0,
        }
    }

    /// Check if the heartbeat is enabled
    pub fn is_heartbeat_enabled(&self) -> bool {
        // match self.node_state {
        //     NodeState::BackupValidator | NodeState::RunningValidator | NodeState::Outgoing => true,
        //     NodeState::Passive => false,
        // }
        return true;
    }

    pub fn set_current_epoch(&mut self, epoch_index: EpochIndex) {
        self.current_epoch = epoch_index;
    }
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

    // Get our node state from the block stream
    let mut sc_block_stream = Box::pin(sc_block_stream);
    while let Some(result_block_header) = sc_block_stream.next().await {
        match result_block_header {
            Ok(block_header) => {
                // TODO: Optimise this so it's not run every block

                let my_account_data = state_chain_client
                    .get_account_data(&block_header)
                    .await
                    .unwrap();

                let current_epoch = duty_manager.read().await.current_epoch;
                println!("Current epoch is: {}", current_epoch);
                println!("My account data: {:?}", my_account_data);
                let outgoing_or_current_validator = my_account_data.last_active_epoch.is_some()
                    && ((my_account_data.last_active_epoch.unwrap() + 1) == current_epoch
                        || my_account_data.last_active_epoch.unwrap() == current_epoch);
                if outgoing_or_current_validator {
                    println!("We are outgoing or current validator");
                    duty_manager.write().await.node_state = NodeState::Running;

                    let vaults = state_chain_client
                        .get_vaults(&block_header, current_epoch)
                        .await;

                    println!("here are the vaults: {:?}", vaults);
                } else {
                    match my_account_data.state {
                        ChainflipAccountState::Backup => {
                            duty_manager.write().await.node_state = NodeState::HeartbeatAndWatch;
                        }
                        ChainflipAccountState::Passive => {
                            duty_manager.write().await.node_state = NodeState::Passive;
                        }
                        ChainflipAccountState::Validator => {
                            panic!("We should never get here, as this state should be captured in the above `if`");
                        }
                    }
                }
            }
            Err(error) => {
                slog::error!(logger, "Failed to decode block header: {}", error,);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::settings::Settings;
    use crate::state_chain::client::connect_to_state_chain;
    use crate::{logging, settings};
    use std::sync::Arc;

    use super::*;

    #[tokio::test]
    #[ignore = "depends on sc"]
    async fn debug() {
        // let settings = settings::test_utils::new_test_settings().unwrap();
        let settings = Settings::from_file("config/Local.toml").unwrap();
        let logger = logging::test_utils::new_test_logger();

        let (state_chain_client, block_stream) =
            connect_to_state_chain(&settings.state_chain).await.unwrap();

        let duty_manager = Arc::new(RwLock::new(DutyManager::new_test()));

        let duty_manager_fut = start_duty_manager(
            duty_manager.clone(),
            state_chain_client,
            block_stream,
            &logger,
        );

        tokio::join!(duty_manager_fut,);
    }

    // #[test]
    // fn test_active_validator_window() {
    //     let active_window = BlockHeightWindow {
    //         from: 10,
    //         to: Some(20),
    //     };
    //     assert!(active_validator_at(&active_window, 15));
    //     assert!(active_validator_at(&active_window, 20));
    //     assert!(active_validator_at(&active_window, 10));
    //     assert!(!active_validator_at(&active_window, 1));
    //     assert!(!active_validator_at(&active_window, 21));
    //     let active_window = BlockHeightWindow {
    //         from: 100,
    //         to: None,
    //     };
    //     assert!(!active_validator_at(&active_window, 50));
    //     assert!(active_validator_at(&active_window, 150));
    // }

    // #[tokio::test]
    // async fn test_is_active_validator_at() {
    //     let duty_manager = Arc::new(RwLock::new(DutyManager::new_test()));

    //     let mut dm = duty_manager.write().await;
    //     assert!(!dm.is_active_validator_at(ChainId::Ethereum, 0));
    //     dm.active_windows
    //         .push((ChainId::Ethereum, BlockHeightWindow { from: 0, to: None }));
    //     assert!(dm.is_active_validator_at(ChainId::Ethereum, 0));
    //     assert!(dm.is_active_validator_at(ChainId::Ethereum, 100000));

    //     dm.active_windows.clear();
    //     dm.active_windows.push((
    //         ChainId::Ethereum,
    //         BlockHeightWindow {
    //             from: 10,
    //             to: Some(20),
    //         },
    //     ));
    //     assert!(!dm.is_active_validator_at(ChainId::Ethereum, 9));
    //     assert!(dm.is_active_validator_at(ChainId::Ethereum, 10));
    //     assert!(dm.is_active_validator_at(ChainId::Ethereum, 20));
    //     assert!(!dm.is_active_validator_at(ChainId::Ethereum, 21));
    // }
}
