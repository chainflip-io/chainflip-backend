//! Implements support for the witnesser api module
use super::{auction::Auction, staking::Staking};
use codec::Encode;
use substrate_subxt::{module, system::System, Call};

type EthTransactionHash = [u8; 32];

#[module]
pub trait WitnesserApi: System + Staking + Auction {}

#[derive(Clone, Debug, PartialEq, Call, Encode)]
pub struct WitnessStakedCall<T: WitnesserApi> {
    staker_account_id: <T as System>::AccountId,
    amount: <T as Staking>::TokenAmount,
    tx_hash: EthTransactionHash,
}

#[derive(Clone, Debug, PartialEq, Call, Encode)]
pub struct WitnessClaimedCall<T: WitnesserApi> {
    account_id: <T as System>::AccountId,
    amount: <T as Staking>::TokenAmount,
    tx_hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Call, Encode)]
pub struct WitnessAuctionConfirmationCall<T: WitnesserApi> {
    auction_index: <T as Auction>::AuctionIndex,
}
