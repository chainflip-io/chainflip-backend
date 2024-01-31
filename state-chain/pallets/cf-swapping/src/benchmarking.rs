#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_chains::{address::EncodedAddress, benchmarking_value::BenchmarkValue};
use cf_traits::{AccountRoleRegistry, Chainflip};
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{OnNewAccount, UnfilteredDispatchable},
};
use frame_system::RawOrigin;

#[benchmarks]
mod benchmarks {
	use super::*;
	use sp_std::vec;

	#[benchmark]
	fn request_swap_deposit_address() {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		assert_ok!(T::AccountRoleRegistry::register_as_broker(&caller));

		let origin = RawOrigin::Signed(caller);
		let call = Call::<T>::request_swap_deposit_address {
			source_asset: Asset::Eth,
			destination_asset: Asset::Usdc,
			destination_address: EncodedAddress::benchmark_value(),
			broker_commission_bps: 0,
			channel_metadata: None,
		};
		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin.into()));
		}
	}

	#[benchmark]
	fn withdraw() {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		assert_ok!(T::AccountRoleRegistry::register_as_broker(&caller));
		EarnedBrokerFees::<T>::insert(caller.clone(), Asset::Eth, 200);

		#[extrinsic_call]
		withdraw(RawOrigin::Signed(caller.clone()), Asset::Eth, EncodedAddress::benchmark_value());
	}

	#[benchmark]
	fn register_as_broker() {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);

		#[extrinsic_call]
		register_as_broker(RawOrigin::Signed(caller.clone()));

		T::AccountRoleRegistry::ensure_broker(RawOrigin::Signed(caller).into())
			.expect("Caller should be registered as broker");
	}

	#[benchmark]
	fn schedule_swap_from_contract() {
		let deposit_amount = 1_000;

		let witness_origin = T::EnsureWitnessed::try_successful_origin().unwrap();
		let call = Call::<T>::schedule_swap_from_contract {
			from: Asset::Usdc,
			to: Asset::Eth,
			deposit_amount,
			destination_address: EncodedAddress::benchmark_value(),
			tx_hash: [0; 32],
		};

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(witness_origin));
		}

		assert_eq!(
			SwapQueue::<T>::get(),
			vec![Swap::new(
				1,
				Asset::Usdc,
				Asset::Eth,
				deposit_amount,
				SwapType::Swap(ForeignChainAddress::benchmark_value())
			)]
		);
	}

	#[benchmark]
	fn ccm_deposit() {
		let origin = T::EnsureWitnessed::try_successful_origin().unwrap();
		let deposit_metadata = CcmDepositMetadata {
			source_chain: ForeignChain::Ethereum,
			source_address: Some(ForeignChainAddress::benchmark_value()),
			channel_metadata: CcmChannelMetadata {
				message: vec![0x00].try_into().unwrap(),
				gas_budget: 1,
				cf_parameters: Default::default(),
			},
		};
		let call = Call::<T>::ccm_deposit {
			source_asset: Asset::Usdc,
			deposit_amount: 1_000,
			destination_asset: Asset::Eth,
			destination_address: EncodedAddress::benchmark_value(),
			deposit_metadata,
			tx_hash: Default::default(),
		};

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert_eq!(
			SwapQueue::<T>::get(),
			vec![
				Swap::new(1, Asset::Usdc, Asset::Eth, 1_000 - 1, SwapType::CcmPrincipal(1)),
				Swap::new(2, Asset::Usdc, Asset::Eth, 1, SwapType::CcmGas(1))
			]
		);
	}

	#[benchmark]
	fn set_maximum_swap_amount() {
		let asset = Asset::Eth;
		let amount = 1_000;
		let call = Call::<T>::set_maximum_swap_amount { asset, amount: Some(amount) };

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(
				<T as Chainflip>::EnsureGovernance::try_successful_origin().unwrap()
			));
		}

		assert_eq!(crate::MaximumSwapAmount::<T>::get(asset), Some(amount));
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
