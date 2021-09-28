//! Implements support for the witnesser api module
use std::marker::PhantomData;

use super::{
    auction::Auction,
    ethereum_signer::{CeremonyId, EthereumSigner},
    ethereum_broadcaster::{BroadcastAttemptId},
    staking::{FlipBalance, Staking},
};
use codec::Encode;
use pallet_cf_staking::EthereumAddress;
use sp_runtime::AccountId32;
use substrate_subxt::{module, system::System, Call};

type EthTransactionHash = [u8; 32];

#[module]
pub trait WitnesserApi: System + Staking + Auction + EthereumSigner {}

#[derive(Clone, Debug, PartialEq, Call, Encode)]
pub struct WitnessStakedCall<T: WitnesserApi> {
    staker_account_id: AccountId32,
    amount: u128,
    withdrawal_address: Option<EthereumAddress>,
    tx_hash: EthTransactionHash,
    _runtime: PhantomData<T>,
}

#[derive(Clone, Debug, PartialEq, Call, Encode)]
pub struct WitnessClaimedCall<T: WitnesserApi> {
    account_id: <T as System>::AccountId,
    amount: FlipBalance,
    tx_hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Call, Encode)]
pub struct WitnessAuctionConfirmationCall<T: WitnesserApi> {
    auction_index: <T as Auction>::AuctionIndex,
}

#[derive(Clone, Debug, PartialEq, Call, Encode)]
pub struct WitnessEthSignatureSuccessCall<T: WitnesserApi> {
    ceremony_id: CeremonyId,
    signature: cf_chains::eth::SchnorrVerificationComponents,
    _runtime: PhantomData<T>,
}

#[derive(Clone, Debug, PartialEq, Call, Encode)]
pub struct WitnessEthSignatureFailedCall<T: WitnesserApi> {
    ceremony_id: CeremonyId,
    offenders: Vec<<T as System>::AccountId>,
    _runtime: PhantomData<T>,
}

#[derive(Clone, Debug, PartialEq, Call, Encode)]
pub struct WitnessEthBroadcastSuccessCall<T: WitnesserApi> {
    broadcast_attempt_id: BroadcastAttemptId,
    tx_hash: [u8; 32],
    _runtime: PhantomData<T>,
}

#[derive(Clone, Debug, PartialEq, Call, Encode)]
pub struct WitnessEthBroadcastFailureCall<T: WitnesserApi> {
    broadcast_attempt_id: BroadcastAttemptId,
    failure: pallet_cf_broadcast::BroadcastFailure,
    tx_hash: [u8; 32],
    _runtime: PhantomData<T>,
}
