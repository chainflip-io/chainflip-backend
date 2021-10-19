use crate::p2p::AccountId;
//use crate::state_chain::sc_event;
use crate::types::chain::Chain;
use slog::o;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use substrate_subxt::{Client, EventSubscription};

use sc_client_api::notifications::StorageChangeSet;

use crate::logging::COMPONENT_KEY;
//use crate::state_chain::pallets::validator::NewEpochEvent;
//use crate::state_chain::runtime::StateChainRuntime;

pub enum NodeState {
    // Only monitoring for storage change events
    Passive,
    // On the Backup Validator list
    // Only submits heartbeats + monitors storage change events
    BackupValidator,
    // Was/Is on the Active Validator list
    // Uses the active_windows to filter witnessing.
    RunningValidator,
}

// Place holder, get this enum from the SC once its implemented
pub enum ChainflipAccountState {
    Passive,
    Retired,
    Backup,
    Validator,
}

#[derive(Clone)]
pub struct BlockHeightWindow {
    pub from: u64,
    pub to: Option<u64>,
}

impl BlockHeightWindow {
    // pub fn new() -> BlockHeightWindow {
    //     BlockHeightWindow {
    //         from: None,
    //         to: None,
    //     }
    // }
    pub fn active_validator_at(&self, block_number: u64) -> bool {
        let in_lower_window = self.from <= block_number;
        let in_upper_window = match self.to {
            Some(to_block) => to_block >= block_number,
            None => true, // if no upper limit exists, then pass the check
        };
        in_lower_window && in_upper_window
    }
}

pub struct DutyManager {
    account_id: AccountId,
    node_state: NodeState,
    _account_state: ChainflipAccountState,
    active_windows: HashMap<Chain, BlockHeightWindow>,
}

impl DutyManager {
    pub fn new(account_id: AccountId) -> DutyManager {
        DutyManager {
            account_id,
            node_state: NodeState::Passive,
            _account_state: ChainflipAccountState::Passive,
            active_windows: HashMap::new(),
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
            active_windows: HashMap::new(),
        }
    }

    /// Check if the heartbeat is enabled
    pub fn is_heartbeat_enabled(&self) -> bool {
        match self.node_state {
            NodeState::BackupValidator | NodeState::RunningValidator => true,
            NodeState::Passive => false,
        }
    }

    /// check if the sc observer is observing all events or just the KeygenRequestEvent
    pub fn is_sc_observation_enabled(&self) -> bool {
        match self.node_state {
            NodeState::RunningValidator => true,
            NodeState::BackupValidator | NodeState::Passive => false,
        }
    }

    /// check if our account id is in the provided list
    pub fn account_list_contains_account_id(&self, account_list: &Vec<AccountId>) -> bool {
        account_list.contains(&self.account_id)
    }

    /// Check if we were an active validator at a specified block number and increments total_witnessed_events if true
    pub fn is_active_validator_at(&self, chain: Chain, block_number: u64) -> bool {
        match self.node_state {
            NodeState::RunningValidator => match self.active_windows.get(&chain) {
                Some(active_window) if active_window.active_validator_at(block_number) => true,
                _ => false,
            },
            _ => false,
        }
    }

    fn process_storage_change(&mut self, set: StorageChangeSet) {
        // use the StorageChangeSet to change the state and active windows
        for change in set.iter() {
            match change {
                // Some(key) => {

                // }
                _ => {}
            }
        }
    }
}

pub async fn start_duty_manager(
    duty_manager: Arc<RwLock<DutyManager>>,
    //subxt_client: Client<StateChainRuntime>,
    logger: &slog::Logger,
) {
    let logger = logger.new(o!(COMPONENT_KEY => "DutyManager"));
    slog::info!(logger, "Starting");

    // TODO: wait for SC sync

    // TODO: How do we subscribe to storage updates on the account map?
    let mut sub = EventSubscription::new(
        subxt_client
            .subscribe_finalized_events()
            .await
            .expect("Could not subscribe to state chain events"),
        subxt_client.events_decoder(),
    );
    while let Some(res_event) = sub.next().await {
        let raw_event = match res_event {
            Ok(raw_event) => raw_event,
            Err(e) => {
                slog::error!(
                    logger,
                    "Next event could not be read from subxt subscription: {}",
                    e
                );
                continue;
            }
        };

        // TODO: need to make an sc_event for the StorageChangeSet
        match sc_event::raw_event_to_sc_event(&raw_event)
            .expect("Could not convert substrate event to SCEvent")
        {
            Some(sc_event) => {
                match sc_event {
                    // send events from the stream to process_storage_change()
                    _ => {} // wip
                }
            }
            _ => { /* wip */ }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{logging, settings};
    use substrate_subxt::ClientBuilder;

    use crate::common::Mutex;
    use sp_keyring::AccountKeyring;
    use std::sync::Arc;
    use substrate_subxt::PairSigner;

    use super::*;

    #[tokio::test]
    #[ignore = "depends on sc"]
    async fn debug() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let logger = logging::test_utils::create_test_logger();

        // let subxt_client = ClientBuilder::<StateChainRuntime>::new()
        //     .set_url(&settings.state_chain.ws_endpoint)
        //     .build()
        //     .await
        //     .expect("Should create subxt client");

        //let alice = AccountKeyring::Alice.pair();
        //let pair_signer = Arc::new(Mutex::new(PairSigner::new(alice)));

        let duty_manager = Arc::new(RwLock::new(DutyManager::new_test()));

        let duty_manager_fut = start_duty_manager(duty_manager.clone(), &logger);

        tokio::join!(
            duty_manager_fut,
            // heartbeat::start(
            //     subxt_client.clone(),
            //     pair_signer.clone(),
            //     &logger,
            //     duty_manager.clone()
            // )
        );
    }

    #[test]
    fn test_active_validator_window() {
        let active_window = BlockHeightWindow {
            from: 10,
            to: Some(20),
        };
        assert!(active_window.active_validator_at(15));
        assert!(active_window.active_validator_at(20));
        assert!(active_window.active_validator_at(10));
        assert!(!active_window.active_validator_at(1));
        assert!(!active_window.active_validator_at(21));
        let active_window = BlockHeightWindow {
            from: 100,
            to: None,
        };
        assert!(!active_window.active_validator_at(50));
        assert!(active_window.active_validator_at(150));
    }
}
