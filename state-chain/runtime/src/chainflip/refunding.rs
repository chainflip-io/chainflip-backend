use crate::Refunding;

use cf_traits::Refunding as RefundingTrait;

use cf_chains::{
	address::IntoForeignChainAddress, btc::ScriptPubkey, dot::PolkadotAccountId, Arbitrum, Bitcoin,
	Ethereum, Polkadot, Solana,
};
use cf_primitives::AssetAmount;

use cf_chains::ForeignChain;
use pallet_cf_refunding::{RecordedFees, WithheldTransactionFees};

use crate::{eth::Address as EvmAddress, Runtime};
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
			// Returns the amount of withheld transaction fees for a chain
			fn get_withheld_transaction_fees(asset: T::ChainAsset) -> AssetAmount {
				let chain: ForeignChain = asset.into();
				WithheldTransactionFees::<Runtime>::get(chain).into()
			}
			// Returns the number of stored records for a chain
			fn get_recorded_gas_fees(asset: T::ChainAsset) -> u128 {
				let chain: ForeignChain = asset.into();
				RecordedFees::<Runtime>::get(chain)
					.expect("No recorded fees for chain")
					.values()
					.len() as u128
			}
		}
	};
}

impl_refunding!(EthRefunding, Ethereum, EvmAddress);
impl_refunding!(BtcRefunding, Bitcoin, ScriptPubkey);
impl_refunding!(DotRefunding, Polkadot, PolkadotAccountId);
impl_refunding!(SolRefunding, Solana, SolAddress);
impl_refunding!(ArbRefunding, Arbitrum, EvmAddress);
