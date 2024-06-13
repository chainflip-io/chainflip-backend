use crate::Refunding;

use cf_traits::Refunding as RefundingTrait;

use cf_chains::{
	address::IntoForeignChainAddress, btc::ScriptPubkey, dot::PolkadotAccountId, Arbitrum, Bitcoin,
	Ethereum, Polkadot, Solana,
};

use crate::eth::Address as EvmAddress;
use cf_chains::sol::SolAddress;

macro_rules! impl_refunding {
	($name:ident, $chain:ident, $account:ident) => {
		pub struct $name<Chain> {
			phantom: sp_std::marker::PhantomData<Chain>,
		}
		impl<T: cf_chains::Chain<ChainAccount = $account>> RefundingTrait<T> for $name<$chain> {
			fn record_gas_fees(
				account_id: T::ChainAccount,
				asset: T::ChainAsset,
				amount: T::ChainAmount,
			) {
				let address =
					<$account as IntoForeignChainAddress<$chain>>::into_foreign_chain_address(
						account_id,
					);
				Refunding::record_gas_fee(address, asset.into(), amount.into());
			}
			fn with_held_transaction_fees(asset: T::ChainAsset, amount: T::ChainAmount) {
				Refunding::withheld_transaction_fee(asset.into(), amount.into());
			}
		}
	};
}

impl_refunding!(EthRefunding, Ethereum, EvmAddress);
impl_refunding!(BtcRefunding, Bitcoin, ScriptPubkey);
impl_refunding!(DotRefunding, Polkadot, PolkadotAccountId);
impl_refunding!(SolRefunding, Solana, SolAddress);
impl_refunding!(ArbRefunding, Arbitrum, EvmAddress);
