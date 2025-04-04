
use cf_chains::address::EncodedAddress;
use sp_core::{
	serde::{Deserialize, Serialize},
	H256, U256, Encode, Decode,
	crypto::AccountId32,
};
use sp_std::vec::Vec;
use frame_support::pallet_prelude::TypeInfo;
use cf_chains::{Chain, ChainCrypto, ChannelRefundParameters, ForeignChain};
use cf_primitives::{AccountRole, Affiliates, Asset, BasisPoints, ChannelId, SemVer, BroadcastId};

pub type TransactionInIdFor<C> = <<C as Chain>::ChainCrypto as ChainCrypto>::TransactionInId;

#[derive(Serialize, Deserialize)]
pub enum TransactionInId {
	Bitcoin(TransactionInIdFor<cf_chains::Bitcoin>),
	Ethereum(TransactionInIdFor<cf_chains::Ethereum>),
	Arbitrum(TransactionInIdFor<cf_chains::Arbitrum>),
	// other variants reserved for other chains.
}

#[derive(Serialize, Deserialize)]
pub enum GetOpenDepositChannelsQuery {
	All,
	Mine,
}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct ChainAccounts {
	pub chain_accounts: Vec<EncodedAddress>,
}

#[derive(
	Serialize,
	Deserialize,
	Encode,
	Decode,
	Eq,
	PartialEq,
	TypeInfo,
	Debug,
	Clone,
	Copy,
	PartialOrd,
	Ord,
)]
pub enum ChannelActionType {
	Swap,
	LiquidityProvision,
}

// impl<AccountId> From<ChannelAction<AccountId>> for ChannelActionType {
// 	fn from(action: ChannelAction<AccountId>) -> Self {
// 		match action {
// 			ChannelAction::Swap { .. } => ChannelActionType::Swap,
// 			ChannelAction::LiquidityProvision { .. } => ChannelActionType::LiquidityProvision,
// 		}
// 	}
// }

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub enum TransactionScreeningEvent<AccountId, TxId> {
	TransactionRejectionRequestReceived {
		account_id: AccountId,
		tx_id: TxId,
	},

	TransactionRejectionRequestExpired {
		account_id: AccountId,
		tx_id: TxId,
	},

	TransactionRejectedByBroker {
		refund_broadcast_id: BroadcastId,
		tx_id: TxId,
	},
}

pub type BrokerRejectionEventFor<AccountId, C> =
	TransactionScreeningEvent<AccountId, <<C as Chain>::ChainCrypto as ChainCrypto>::TransactionInId>;

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct TransactionScreeningEvents<AccountId> {
	pub btc_events: Vec<BrokerRejectionEventFor<AccountId, cf_chains::Bitcoin>>,
	pub eth_events: Vec<BrokerRejectionEventFor<AccountId, cf_chains::Ethereum>>,
	pub arb_events: Vec<BrokerRejectionEventFor<AccountId, cf_chains::Arbitrum>>,
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone)]
pub struct VaultAddresses {
	pub ethereum: EncodedAddress,
	pub arbitrum: EncodedAddress,
	pub bitcoin: Vec<(AccountId32, EncodedAddress)>,
}