use std::collections::BTreeMap;

use crate::Refunding;
use cf_chains::{AnyChain, Bitcoin};

pub struct MockRefunding<Chain> {
	phantom: sp_std::marker::PhantomData<Chain>,
}

impl<T: cf_chains::Chain> Refunding<T> for MockRefunding<AnyChain> {
	fn record_gas_fees(account_id: T::ChainAccount, asset: T::ChainAsset, amount: T::ChainAmount) {}
	fn with_held_transaction_fees(asset: T::ChainAsset, amount: T::ChainAmount) {}
}
