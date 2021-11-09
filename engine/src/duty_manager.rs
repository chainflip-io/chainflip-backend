//! The DutyManager contains logic that allows for enabling and disabling of features
//! within the CFE depending on its state and the block heights of each respective blockchain.

use crate::state_chain::client::{StateChainClient, StateChainRpcApi};
use cf_chains::ChainId;
use pallet_cf_vaults::BlockHeightWindow;
use std::{collections::HashMap, sync::Arc};

use cf_traits::ChainflipAccountData;

/// Represents the different "action" states the CFE can be in
/// These only have rough mappings to the State Chain's idea of a node's state
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum NodeState {
    // Only monitoring for storage change events - so we know when/if we should transition to another state
    Passive,
    // Only submits heartbeats + monitors storage change events
    // Backup Validators and Outgoing Validators (which may be Backup too) fall into this category
    Backup,

    // Represents a "running" state, but one that will be transitioned out of
    // once the chains are caught up to their end blocks
    Outgoing,

    // Is on the Active Validator list
    // Uses the active_windows to filter witnessing.
    // Outgoing, until the last ETH block is done being witnessed - do we want to cache this somewhere? - or use a Last consensus block method
    Active,
}

#[derive(Debug)]
pub struct DutyManager {
    node_state: NodeState,
    /// Contains the block at which we start our validator duties for each respective chain
    active_windows: Option<HashMap<ChainId, BlockHeightWindow>>,
}

impl DutyManager {
    pub fn new(node_state: NodeState) -> DutyManager {
        DutyManager {
            node_state,
            active_windows: None,
        }
    }

    /// Creates a Duty Manager in RunningValidator state with 0's account ID
    #[cfg(test)]
    pub fn new_test() -> DutyManager {
        let test_account_id: [u8; 32] = [0; 32];
        DutyManager {
            node_state: NodeState::Active,
            active_windows: None,
        }
    }

    /// Check if the heartbeat is enabled
    pub fn is_heartbeat_enabled(&self) -> bool {
        match self.node_state {
            NodeState::Backup | NodeState::Active | NodeState::Outgoing => true,
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

    pub async fn update_active_window_for_chain<RpcClient: StateChainRpcApi>(
        &mut self,
        chain_id: ChainId,
        account_data: ChainflipAccountData,
        state_chain_client: Arc<StateChainClient<RpcClient>>,
    ) {
        let eth_vault = state_chain_client
            .get_vault(
                None,
                account_data.last_active_epoch.expect("guarded above"),
                ChainId::Ethereum,
            )
            .await
            .expect("should pass");

        if let Some(active_windows) = self.active_windows.as_mut() {
            active_windows.insert(chain_id, eth_vault.active_window);
        } else {
            let mut map = HashMap::new();
            map.insert(chain_id, eth_vault.active_window);
            self.active_windows = Some(map);
        };
    }

    pub fn set_node_state(&mut self, node_state: NodeState) {
        self.node_state = node_state;
    }

    pub fn get_node_state(&self) -> NodeState {
        self.node_state
    }
}

#[cfg(test)]
mod tests {
    use crate::settings::Settings;
    use crate::state_chain::client::connect_to_state_chain;
    use crate::{logging, settings};
    use std::sync::Arc;

    use super::*;

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
