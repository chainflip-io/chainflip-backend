use serde::{Serialize, Deserialize};
use web3::types::Address;

pub(crate) mod set_agg_key_with_agg_key;

/// Details of a contract call to be broadcast to ethereum.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ContractCallDetails {
    pub contract_address: Address,
    pub data: Vec<u8>,
}
