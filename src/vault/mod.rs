pub mod blockchain_connection;
pub mod side_chain;
pub mod witness;

use std::sync::{Arc, Mutex};

pub use side_chain::SideChain;

pub struct Vault {
    _side_chain: Arc<Mutex<SideChain>>,
}

impl Vault {
    pub fn new(side_chain: Arc<Mutex<SideChain>>) -> Vault {
        Vault {
            _side_chain: side_chain,
        }
    }
}
