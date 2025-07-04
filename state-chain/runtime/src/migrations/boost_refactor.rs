// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use cf_chains::instances::{
	ArbitrumInstance, AssethubInstance, BitcoinInstance, EthereumInstance, PolkadotInstance,
	SolanaInstance,
};
use cf_primitives::Asset;
use codec::{Decode, Encode};
use frame_support::{traits::UncheckedOnRuntimeUpgrade, weights::Weight, *};
use sp_runtime::{Percent, TryRuntimeError};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

pub struct BoostRefactorMigration;
use crate::{AccountId, Runtime};
use pallet_cf_lending_pools::ScaledAmount;

mod old {

	use sp_core::RuntimeDebug;
	use sp_runtime::AccountId32;

	use super::*;

	use pallet_cf_lending_pools::migration_support::old::BoostPool;

	use cf_primitives::{BasisPoints, BoostPoolTier, PrewitnessedDepositId};
	use codec::{Decode, Encode};
	use frame_system::pallet_prelude::BlockNumberFor;
	use pallet_cf_ingress_egress::{ChannelAction, TargetChainAsset};
	use scale_info::TypeInfo;

	use cf_chains::{Chain, DepositChannel};

	pub(crate) type TargetChainAmount<T, I> =
		<<T as pallet_cf_ingress_egress::Config<I>>::TargetChain as Chain>::ChainAmount;
	pub(crate) type TargetChainBlockNumber<T, I> =
		<<T as pallet_cf_ingress_egress::Config<I>>::TargetChain as Chain>::ChainBlockNumber;
	use pallet_cf_ingress_egress::{TargetChainAccount, TransactionInIdFor};

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, Default)]
	pub enum BoostStatus<ChainAmount, BlockNumber> {
		Boosted {
			prewitnessed_deposit_id: PrewitnessedDepositId,
			// This is to be removed:
			pools: Vec<BoostPoolTier>,
			amount: ChainAmount,
		},
		#[default]
		NotBoosted,
		BoostPending {
			amount: ChainAmount,
			process_at_block: BlockNumber,
		},
	}

	// This contains boost status which needs to be migrated:
	#[frame_support::storage_alias]
	pub type DepositChannelLookup<T: pallet_cf_ingress_egress::Config<I>, I: 'static> = StorageMap<
		pallet_cf_ingress_egress::Pallet<T, I>,
		Twox64Concat,
		TargetChainAccount<T, I>,
		DepositChannelDetails<T, I>,
	>;

	#[derive(CloneNoBound, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct DepositChannelDetails<T: pallet_cf_ingress_egress::Config<I>, I: 'static> {
		/// The owner of the deposit channel.
		pub owner: T::AccountId,
		pub deposit_channel: DepositChannel<T::TargetChain>,
		/// The block number at which the deposit channel was opened, expressed as a block number
		/// on the external Chain.
		pub opened_at: TargetChainBlockNumber<T, I>,
		/// The last block on the target chain that the witnessing will witness it in. If funds are
		/// sent after this block, they will not be witnessed.
		pub expires_at: TargetChainBlockNumber<T, I>,
		/// The action to be taken when the DepositChannel is deposited to.
		pub action: ChannelAction<T::AccountId, T::TargetChain>,
		/// The boost fee
		pub boost_fee: BasisPoints,
		/// Boost status, indicating whether there is pending boost on the channel
		pub boost_status: BoostStatus<TargetChainAmount<T, I>, BlockNumberFor<T>>,
	}

	// This contains boost status which needs to be migrated:
	#[rustfmt::skip] // prevents a trailing comma wihch seems to break the macro
	#[frame_support::storage_alias]
	pub(crate) type BoostedVaultTransactions<
		T: pallet_cf_ingress_egress::Config<I>,
		I: 'static
	> = StorageMap<
		pallet_cf_ingress_egress::Pallet<T, I>,
		Identity,
		TransactionInIdFor<T, I>,
		BoostStatus<TargetChainAmount<T, I>, BlockNumberFor<T>>,
	>;

	#[frame_support::storage_alias]
	pub type BoostPools<T: pallet_cf_ingress_egress::Config<I>, I: 'static> = StorageDoubleMap<
		pallet_cf_ingress_egress::Pallet<T, I>,
		Twox64Concat,
		TargetChainAsset<Runtime, I>,
		Twox64Concat,
		BoostPoolTier,
		BoostPool<AccountId32>,
	>;

	#[rustfmt::skip] // prevents a trailing comma wihch seems to break the macro
	#[frame_support::storage_alias]
	pub type NetworkFeeDeductionFromBoostPercent<
		T: pallet_cf_ingress_egress::Config<I>,
		I: 'static
	> = StorageValue<pallet_cf_ingress_egress::Pallet<T, I>, Percent>;
}

// Used to store a summary of the state before the runtime upgrade to
// verify post-upgrade state against
#[derive(Encode, Decode)]
struct PreUpgradeData {
	number_of_pools: u32,
	network_fee_deduction: Percent,
	boost_balances: BTreeMap<AccountId, ScaledAmount>,
}

fn migrate_boost_status<AccountId, BlockNumber>(
	status: old::BoostStatus<AccountId, BlockNumber>,
) -> pallet_cf_ingress_egress::BoostStatus<AccountId, BlockNumber> {
	match status {
		old::BoostStatus::NotBoosted => pallet_cf_ingress_egress::BoostStatus::NotBoosted,
		old::BoostStatus::BoostPending { amount, process_at_block } =>
			pallet_cf_ingress_egress::BoostStatus::BoostPending { amount, process_at_block },
		old::BoostStatus::Boosted { prewitnessed_deposit_id, pools: _, amount } =>
			pallet_cf_ingress_egress::BoostStatus::Boosted { prewitnessed_deposit_id, amount },
	}
}

#[cfg(feature = "try-runtime")]
/// Total available balances in BTC boost pools before migration:
fn old_boost_balances() -> BTreeMap<AccountId, ScaledAmount> {
	let mut balances: BTreeMap<AccountId, ScaledAmount> = Default::default();

	for (_, pool) in
		old::BoostPools::<Runtime, BitcoinInstance>::iter_prefix(cf_chains::assets::btc::Asset::Btc)
	{
		for (acc_id, amount) in pool.amounts {
			let entry = balances.entry(acc_id).or_default();
			*entry = *entry + amount;
		}
	}

	balances
}

#[cfg(feature = "try-runtime")]
// Total available balances in boost pools (including what's in ongoing boosts) after migration:
fn new_boost_balances() -> BTreeMap<AccountId, ScaledAmount> {
	let mut balances: BTreeMap<AccountId, ScaledAmount> = Default::default();

	for (_, pool) in pallet_cf_lending_pools::BoostPools::<Runtime>::iter_prefix(Asset::Btc) {
		if let Some(pool) =
			pallet_cf_lending_pools::CorePools::<Runtime>::get(Asset::Btc, pool.core_pool_id)
		{
			for (acc_id, amount) in pool.amounts {
				let entry = balances.entry(acc_id).or_default();
				*entry = *entry + amount;
			}
		}
	}

	balances
}

impl UncheckedOnRuntimeUpgrade for BoostRefactorMigration {
	fn on_runtime_upgrade() -> Weight {
		let pool_ids = old::BoostPools::<Runtime, BitcoinInstance>::iter_keys().collect::<Vec<_>>();

		// Migrate pools
		for (asset, tier, boost_pool) in old::BoostPools::<Runtime, BitcoinInstance>::iter() {
			pallet_cf_lending_pools::migration_support::migrate_boost_pools::<Runtime>(
				asset.into(),
				tier,
				boost_pool,
			);
		}

		// Couldn't get kill() to compile so removing elements individually:
		for (asset, tier) in pool_ids {
			old::BoostPools::<Runtime, BitcoinInstance>::remove(asset, tier);
		}

		// Migrate NetworkFeeDeductionFromBoostPercent (only set for the Bitcoin instance)
		let network_fee_deduction =
			old::NetworkFeeDeductionFromBoostPercent::<Runtime, BitcoinInstance>::get()
				.unwrap_or_default();
		pallet_cf_lending_pools::NetworkFeeDeductionFromBoostPercent::<Runtime>::set(
			network_fee_deduction,
		);

		// Remove unnecessary fields from the boost status on channels (only necessary for the
		// Bitcoin instance since for all other assets this should be NotBoosted, which should
		// still be decoded correctly:
		pallet_cf_ingress_egress::BoostedVaultTransactions::<Runtime, BitcoinInstance>::translate_values(
			|boost_status : old::BoostStatus<_,_>| {
				Some(migrate_boost_status(boost_status))
			},
		);

		old::NetworkFeeDeductionFromBoostPercent::<Runtime, EthereumInstance>::kill();
		old::NetworkFeeDeductionFromBoostPercent::<Runtime, PolkadotInstance>::kill();
		old::NetworkFeeDeductionFromBoostPercent::<Runtime, BitcoinInstance>::kill();
		old::NetworkFeeDeductionFromBoostPercent::<Runtime, ArbitrumInstance>::kill();
		old::NetworkFeeDeductionFromBoostPercent::<Runtime, SolanaInstance>::kill();
		old::NetworkFeeDeductionFromBoostPercent::<Runtime, AssethubInstance>::kill();

		Default::default()
	}

	/// See [`Hooks::pre_upgrade`].
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		let number_of_pools = old::BoostPools::<Runtime, BitcoinInstance>::iter().count() as u32;
		let network_fee_deduction =
			old::NetworkFeeDeductionFromBoostPercent::<Runtime, BitcoinInstance>::get()
				.unwrap_or_default();

		let data = PreUpgradeData {
			number_of_pools,
			network_fee_deduction,
			boost_balances: old_boost_balances(),
		};

		Ok(data.encode())
	}

	/// See [`Hooks::post_upgrade`].
	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
		let PreUpgradeData { number_of_pools, network_fee_deduction, boost_balances } =
			PreUpgradeData::decode(&mut state.as_slice()).unwrap();

		assert_eq!(
			pallet_cf_lending_pools::CorePools::<Runtime>::iter().count(),
			number_of_pools as usize
		);

		assert_eq!(
			pallet_cf_lending_pools::BoostPools::<Runtime>::iter().count(),
			number_of_pools as usize
		);

		assert_eq!(new_boost_balances(), boost_balances);

		assert_eq!(pallet_cf_lending_pools::NextCorePoolId::<Runtime>::get().0, number_of_pools);

		assert_eq!(
			pallet_cf_lending_pools::NetworkFeeDeductionFromBoostPercent::<Runtime>::get(),
			network_fee_deduction
		);

		Ok(())
	}
}
