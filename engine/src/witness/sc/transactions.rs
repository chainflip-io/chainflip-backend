// Implements support for the transactions module

use codec::{Codec, Decode, Encode};
use substrate_subxt::{
    module,
    sp_runtime::{app_crypto::RuntimePublic, traits::Member},
    system::System,
    Event,
};

#[module]
pub trait Transactions: System {}

// Apparently should be an event type here
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode)]
pub struct DataAddedEvent<T: Transactions> {
    pub who: <T as System>::AccountId,

    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode)]
pub struct DataAddedMoreEvent<T: Transactions> {
    pub who: <T as System>::AccountId,

    pub data: Vec<u8>,
}


#[cfg(test)]
mod tests {

    use frame_system::RawEvent;

    use super::*;

    #[test]
    fn test_decode_raw_data_added() {
        let raw_data_added = 
    }
}