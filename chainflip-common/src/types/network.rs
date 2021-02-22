use serde::{Deserialize, Serialize};
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Network {
    Mainnet,
    Testnet,
}
