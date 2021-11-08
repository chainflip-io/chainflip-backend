//! The DutyManager contains logic that allows for enabling and disabling of features
//! within the CFE depending on its state and the block heights of each respective blockchain.

use crate::p2p::AccountId;
use cf_chains::ChainId;
use pallet_cf_vaults::{BlockHeight, BlockHeightWindow};
use slog::o;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use cf_traits::{ChainflipAccountData, ChainflipAccountState, EpochIndex};

use crate::logging::COMPONENT_KEY;

/// Represents the different "action" states the CFE can be in
/// These only have rough mappings to the State Chain's idea of a node's state
#[derive(Debug)]
pub enum NodeState {
    // Only monitoring for storage change events - so we know when/if we should transition to another state
    Passive,
    // Only submits heartbeats + monitors storage change events
    // Backup Validators and Outgoing Validators (which may be Backup too) fall into this category
    HeartbeatAndWatch,

    // Is on the Active Validator list
    // Uses the active_windows to filter witnessing.
    // Outgoing, until the last ETH block is done being witnessed - do we want to cache this somewhere? - or use a Last consensus block method
    Running,
}

#[derive(Debug)]
pub struct DutyManager {
    account_id: AccountId,
    node_state: NodeState,
    /// The epoch that the chain is currently in
    current_epoch: EpochIndex,
    /// Contains the block at which we start our validator duties for each respective chain
    active_windows: Option<HashMap<ChainId, BlockHeightWindow>>,
}

impl DutyManager {
    pub fn new(account_id: AccountId, current_epoch: EpochIndex) -> DutyManager {
        DutyManager {
            account_id,
            node_state: NodeState::Passive,
            active_windows: None,
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
            active_windows: None,
            current_epoch: 0,
        }
    }

    /// Check if the heartbeat is enabled
    pub fn is_heartbeat_enabled(&self) -> bool {
        match self.node_state {
            NodeState::HeartbeatAndWatch | NodeState::Running => true,
            NodeState::Passive => false,
        }
    }

    pub fn is_running_validator_for_chain_at(&self, chain: ChainId, block: u64) -> bool {
        if let Some(active_windows) = &self.active_windows {
            let chain_window = active_windows.get(&chain);
            if let Some(window) = chain_window {
                if window.to.is_none() {
                    return true;
                } else {
                    return window.from <= block
                        && block <= window.to.expect("safe due to condition");
                }
            }
        }
        false
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
                // let outgoing_or_current_validator = my_account_data.last_active_epoch.is_some()
                //     && ((my_account_data.last_active_epoch.unwrap() + 1) == current_epoch
                //         || my_account_data.last_active_epoch.unwrap() == current_epoch);

                let outgoing_or_current_validator = true;
                if outgoing_or_current_validator {
                    println!("We are outgoing or current validator");

                    // TODO: use the actual last active epoch after this is in: https://github.com/chainflip-io/chainflip-backend/issues/796
                    let last_active_epoch = 0;

                    // we currently only need the ETH vault
                    let eth_vault = state_chain_client
                        .get_vault(&block_header, last_active_epoch, ChainId::Ethereum)
                        .await
                        .unwrap();

                    let mut active_windows = HashMap::new();
                    active_windows.insert(ChainId::Ethereum, eth_vault.active_window);

                    {
                        let mut w_duty_manager = duty_manager.write().await;
                        w_duty_manager.node_state = NodeState::Running;
                        w_duty_manager.active_windows = Some(active_windows);
                    }
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
