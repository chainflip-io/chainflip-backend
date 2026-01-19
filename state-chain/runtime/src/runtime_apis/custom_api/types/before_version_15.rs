use super::*;

#[derive(Encode, Decode, TypeInfo, DefaultNoBound)]
#[derive_n_functor]
pub struct BrokerInfo<BtcAddress> {
	pub earned_fees: Vec<(Asset, AssetAmount)>,
	pub btc_vault_deposit_address: Option<BtcAddress>,
	pub affiliates: Vec<(AccountId32, AffiliateDetails)>,
	pub bond: AssetAmount,
}

impl<A> From<BrokerInfo<A>> for super::BrokerInfo<A> {
	fn from(old: BrokerInfo<A>) -> Self {
		Self {
			earned_fees: old.earned_fees,
			btc_vault_deposit_address: old.btc_vault_deposit_address,
			affiliates: old.affiliates,
			bond: old.bond,
			..Default::default()
		}
	}
}
