use web3::{signing::keccak256, types::H256};
use ::web3::types::{
    Log,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StakingEvents {
    Staked{ stakerId: H256 },
    Unknown
}

impl StakingEvents {
    const TOPIC_STAKED: H256 = keccak256(b"Staked(uint256)").into();
}


impl From<Log> for StakingEvents {
    fn from(log: Log) -> Self {
        match log.topics.split_first() {
            Some((&t, _)) if t == Self::TOPIC_STAKED => {
                StakingEvents::Staked { stakerId: H256::from_slice(&log.data.0[0..32]) }
            },
            _ => StakingEvents::Unknown
        }
    }
}
