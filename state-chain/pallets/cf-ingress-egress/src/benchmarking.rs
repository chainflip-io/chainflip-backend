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

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{BoostStatus, DisabledEgressAssets};
use cf_chains::{
	benchmarking_value::{BenchmarkValue, BenchmarkValueExtended},
	CcmChannelMetadataUnchecked, ChannelRefundParametersForChain, DepositChannel,
};
use cf_primitives::AccountRole;
use cf_traits::AccountRoleRegistry;
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{OnNewAccount, OriginTrait, UnfilteredDispatchable},
};

pub(crate) type TargetChainBlockNumber<T, I> =
	<<T as Config<I>>::TargetChain as Chain>::ChainBlockNumber;

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	const TIER_5_BPS: BoostPoolTier = 5;

	#[benchmark]
	fn disable_asset_egress() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let destination_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset =
			BenchmarkValue::benchmark_value();

		#[block]
		{
			assert_ok!(Pallet::<T, I>::enable_or_disable_egress(origin, destination_asset, true));
		}

		assert!(DisabledEgressAssets::<T, I>::get(destination_asset,).is_some());
	}

	#[benchmark]
	fn process_channel_deposit_full_witness() {
		const CHANNEL_ID: u64 = 1;

		let deposit_address: <<T as Config<I>>::TargetChain as Chain>::ChainAccount =
			BenchmarkValue::benchmark_value();
		let source_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset =
			BenchmarkValue::benchmark_value();
		let deposit_amount: <<T as Config<I>>::TargetChain as Chain>::ChainAmount =
			BenchmarkValue::benchmark_value();
		let block_number: TargetChainBlockNumber<T, I> = BenchmarkValue::benchmark_value();
		DepositChannelLookup::<T, I>::insert(
			&deposit_address,
			DepositChannelDetails {
				owner: account("doogle", 0, 0),
				opened_at: block_number,
				expires_at: block_number,
				deposit_channel:
					DepositChannel::generate_new::<<T as Config<I>>::AddressDerivation>(
						CHANNEL_ID,
						source_asset,
					)
					.unwrap(),
				action: ChannelAction::<T::AccountId, <T::TargetChain as Chain>::ChainAccount>::LiquidityProvision {
					lp_account: account("doogle", 0, 0),
					refund_address: ForeignChainAddress::benchmark_value(),
				},
				boost_fee: 0,
				boost_status: BoostStatus::NotBoosted,
			},
		);

		#[block]
		{
			assert_ok!(Pallet::<T, I>::process_channel_deposit_full_witness_inner(
				&DepositWitness {
					deposit_address,
					asset: source_asset,
					amount: deposit_amount,
					deposit_details: BenchmarkValue::benchmark_value(),
				},
				BenchmarkValue::benchmark_value()
			));
		}
	}
	#[benchmark]
	fn finalise_ingress(a: Linear<1, 100>) {
		let mut addresses = vec![];
		let origin = T::EnsureWitnessedAtCurrentEpoch::try_successful_origin().unwrap();
		for _ in 1..a {
			let deposit_address =
				<<T as Config<I>>::TargetChain as Chain>::ChainAccount::benchmark_value_by_id(
					a as u8,
				);
			let source_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset =
				BenchmarkValue::benchmark_value();
			let block_number = TargetChainBlockNumber::<T, I>::benchmark_value();
			let mut channel =
				DepositChannelDetails::<T, I> {
					owner: account("doogle", 0, 0),
					opened_at: block_number,
					expires_at: block_number,
					deposit_channel: DepositChannel::generate_new::<
						<T as Config<I>>::AddressDerivation,
					>(1, source_asset)
					.unwrap(),
					action: ChannelAction::<T::AccountId, <T::TargetChain as Chain>::ChainAccount>::LiquidityProvision {
						lp_account: account("doogle", 0, 0),
						refund_address: ForeignChainAddress::benchmark_value(),
					},
					boost_fee: 0,
					boost_status: BoostStatus::NotBoosted,
				};
			channel.deposit_channel.state.on_fetch_scheduled();
			DepositChannelLookup::<T, I>::insert(deposit_address.clone(), channel);
			addresses.push(deposit_address);
		}

		#[block]
		{
			assert_ok!(Pallet::<T, I>::finalise_ingress(origin, addresses));
		}
	}

	#[benchmark]
	fn vault_transfer_failed() {
		let epoch = T::EpochInfo::epoch_index();
		let origin = T::EnsureWitnessedAtCurrentEpoch::try_successful_origin().unwrap();
		let asset: TargetChainAsset<T, I> = BenchmarkValue::benchmark_value();
		let amount: TargetChainAmount<T, I> = BenchmarkValue::benchmark_value();
		let destination_address: TargetChainAccount<T, I> = BenchmarkValue::benchmark_value();

		#[block]
		{
			assert_ok!(Pallet::<T, I>::vault_transfer_failed(
				origin,
				asset,
				amount,
				destination_address.clone()
			));
		}

		assert_eq!(FailedForeignChainCalls::<T, I>::get(epoch).len(), 1);
	}

	#[benchmark]
	fn ccm_broadcast_failed() {
		#[block]
		{
			assert_ok!(Pallet::<T, I>::ccm_broadcast_failed(
				OriginTrait::root(),
				Default::default()
			));
		}

		let current_epoch = T::EpochInfo::epoch_index();
		assert_eq!(
			FailedForeignChainCalls::<T, I>::get(current_epoch),
			vec![FailedForeignChainCall {
				broadcast_id: Default::default(),
				original_epoch: current_epoch
			}]
		);
	}

	fn prewitness_deposit<T: pallet::Config<I>, I>(
		lp_account: &T::AccountId,
		asset: TargetChainAsset<T, I>,
		fee_tier: BoostPoolTier,
	) -> TargetChainAccount<T, I> {
		let (deposit_channel, ..) = Pallet::<T, I>::open_channel(
			lp_account,
			asset,
			ChannelAction::LiquidityProvision {
				lp_account: lp_account.clone(),
				refund_address: ForeignChainAddress::benchmark_value(),
			},
			fee_tier,
		)
		.unwrap();

		assert_ok!(Pallet::<T, I>::process_channel_deposit_prewitness(
			DepositWitness::<T::TargetChain> {
				deposit_address: deposit_channel.address.clone(),
				asset,
				amount: TargetChainAmount::<T, I>::from(1000u32),
				deposit_details: BenchmarkValue::benchmark_value()
			},
			BenchmarkValue::benchmark_value()
		));

		deposit_channel.address
	}

	fn setup_booster_account<T: Config<I>, I>(
		asset: TargetChainAsset<T, I>,
		seed: u32,
	) -> T::AccountId {
		let caller: T::AccountId = account("booster", 0, seed);

		// TODO: remove once https://github.com/chainflip-io/chainflip-backend/pull/4716 is merged
		if frame_system::Pallet::<T>::providers(&caller) == 0u32 {
			frame_system::Pallet::<T>::inc_providers(&caller);
		}
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		assert_ok!(<T as Chainflip>::AccountRoleRegistry::register_as_liquidity_provider(&caller));
		T::Balance::credit_account(&caller, asset.into(), 1_000_000);

		// A non-zero balance is required to pay for the channel opening fee.
		T::FeePayment::mint_to_account(&caller, u32::MAX.into());

		T::Balance::credit_account(&caller, asset.into(), 5_000_000_000_000_000_000u128);

		caller
	}

	#[benchmark]
	fn vault_swap_request() {
		let origin = T::EnsureWitnessed::try_successful_origin().unwrap();
		let deposit_metadata = CcmDepositMetadata {
			source_chain: ForeignChain::Ethereum,
			source_address: Some(ForeignChainAddress::benchmark_value()),
			channel_metadata: CcmChannelMetadataUnchecked {
				message: vec![0x00].try_into().unwrap(),
				gas_budget: 1,
				ccm_additional_data: Default::default(),
			},
		};
		let call = Call::<T, I>::vault_swap_request {
			block_height: 0u32.into(),
			deposit: Box::new(VaultDepositWitness {
				input_asset: BenchmarkValue::benchmark_value(),
				output_asset: Some(Asset::Eth),
				deposit_amount: 1_000u32.into(),
				destination_address: Some(BenchmarkValue::benchmark_value()),
				deposit_metadata: Some(deposit_metadata),
				tx_id: TransactionInIdFor::<T, I>::benchmark_value(),
				deposit_details: BenchmarkValue::benchmark_value(),
				broker_fee: None,
				affiliate_fees: Default::default(),
				refund_params: ChannelRefundParametersForChain::<T::TargetChain> {
					retry_duration: Default::default(),
					refund_address: BenchmarkValue::benchmark_value(),
					min_price: Default::default(),
					refund_ccm_metadata: Default::default(),
				},
				dca_params: None,
				boost_fee: 0,
				channel_id: None,
				deposit_address: None,
			}),
		};

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}
	}

	#[benchmark]
	fn boost_finalised() {
		use strum::IntoEnumIterator;

		let asset = TargetChainAsset::<T, I>::iter().next().unwrap();

		const BOOSTER_COUNT: usize = 100;

		let boosters: Vec<_> = (0..BOOSTER_COUNT)
			.map(|i| setup_booster_account::<T, I>(asset, i as u32))
			.collect();

		let deposit_address = prewitness_deposit::<T, I>(&boosters[0], asset, TIER_5_BPS);

		#[block]
		{
			assert_ok!(Pallet::<T, I>::process_channel_deposit_full_witness_inner(
				&DepositWitness {
					deposit_address,
					asset,
					amount: 1_000u32.into(),
					deposit_details: BenchmarkValue::benchmark_value(),
				},
				BenchmarkValue::benchmark_value()
			));
		}
	}

	#[benchmark]
	fn mark_transaction_for_rejection() {
		let caller =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Broker).unwrap();
		let tx_id: TransactionInIdFor<T, I> = TransactionInIdFor::<T, I>::benchmark_value();

		#[block]
		{
			assert_ok!(Pallet::<T, I>::mark_transaction_for_rejection_inner(
				caller.clone(),
				tx_id.clone(),
			));
		}

		assert!(
			TransactionsMarkedForRejection::<T, I>::get(caller, tx_id).is_some(),
			"No marked transactions found"
		);
	}

	#[cfg(test)]
	use crate::mocks::{new_test_ext, Test};
	#[cfg(test)]
	use frame_support::instances::Instance1;

	#[test]
	fn benchmark_works() {
		new_test_ext().execute_with(|| {
			_ccm_broadcast_failed::<Test, Instance1>(true);
		});
		new_test_ext().execute_with(|| {
			_vault_transfer_failed::<Test, Instance1>(true);
		});
		new_test_ext().execute_with(|| {
			_finalise_ingress::<Test, Instance1>(100, true);
		});
		new_test_ext().execute_with(|| {
			_process_channel_deposit_full_witness::<Test, Instance1>(true);
		});
		new_test_ext().execute_with(|| {
			_disable_asset_egress::<Test, Instance1>(true);
		});
		new_test_ext().execute_with(|| {
			_boost_finalised::<Test, Instance1>(true);
		});
		new_test_ext().execute_with(|| {
			_vault_swap_request::<Test, Instance1>(true);
		});
		new_test_ext().execute_with(|| {
			_vault_swap_request::<Test, Instance1>(true);
		});
		new_test_ext().execute_with(|| {
			_mark_transaction_for_rejection::<Test, Instance1>(true);
		});
	}
}
