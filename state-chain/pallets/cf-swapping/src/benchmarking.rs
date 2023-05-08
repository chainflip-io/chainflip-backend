//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_chains::{address::EncodedAddress, benchmarking_value::BenchmarkValue};
use cf_traits::{AccountRoleRegistry, Chainflip};
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::{dispatch::UnfilteredDispatchable, traits::OnNewAccount};
use frame_system::RawOrigin;

fn generate_swaps<T: Config>(amount: u32, from: Asset, to: Asset) -> Vec<Swap> {
	let mut swaps: Vec<Swap> = vec![];
	for i in 1..amount {
		swaps.push(Swap {
			swap_id: i as u64,
			from,
			to,
			amount: 3,
			swap_type: SwapType::Swap(ForeignChainAddress::benchmark_value()),
		});
	}
	swaps
}

benchmarks! {
	request_swap_deposit_address {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		T::AccountRoleRegistry::register_as_broker(&caller).unwrap();
		let origin = RawOrigin::Signed(caller);
		let call = Call::<T>::request_swap_deposit_address {
			source_asset: Asset::Eth,
			destination_asset: Asset::Usdc,
			destination_address: EncodedAddress::benchmark_value(),
			broker_commission_bps: 0,
			message_metadata: None,
		};
	} : { call.dispatch_bypass_filter(origin.into())?; }

	on_idle {}: {
		Pallet::<T>::on_idle(T::BlockNumber::from(1u32), Weight::from_ref_time(1));
	}

	execute_group_of_swaps {
		// Generate swaps
		let a in 2..150;
		let swaps = generate_swaps::<T>(a, Asset::Eth, Asset::Flip);
	} : {
		let _ = Pallet::<T>::execute_group_of_swaps(&swaps[..], Asset::Eth, Asset::Flip);
	}

	withdraw {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		T::AccountRoleRegistry::register_as_broker(&caller).unwrap();
		EarnedBrokerFees::<T>::insert(caller.clone(), Asset::Eth, 200);
	} : _(
		RawOrigin::Signed(caller.clone()),
		Asset::Eth,
		EncodedAddress::benchmark_value()
	)

	register_as_broker {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
	}: _(RawOrigin::Signed(caller.clone()))
	verify {
		T::AccountRoleRegistry::ensure_broker(RawOrigin::Signed(caller).into())
			.expect("Caller should be registered as broker");
	}

	schedule_swap_by_witnesser {
		let origin = T::EnsureWitnessed::successful_origin();
		let call = Call::<T>::schedule_swap_by_witnesser{
			from: Asset::Usdc,
			to: Asset::Eth,
			deposit_amount: 1_000,
			destination_address: EncodedAddress::benchmark_value()
		};
	}: {
		call.dispatch_bypass_filter(origin)?;
	}
	verify {
		assert_eq!(SwapQueue::<T>::get(), vec![Swap{
			swap_id: 1,
			from: Asset::Usdc,
			to: Asset::Eth,
			amount:1_000,
			swap_type: SwapType::Swap(ForeignChainAddress::benchmark_value())
		}]);
	}

	ccm_deposit {
		let origin = T::EnsureWitnessed::successful_origin();
		let metadata = CcmDepositMetadata {
			message: vec![0x00],
			gas_budget: 1,
			refund_address: ForeignChainAddress::benchmark_value(),
			source_address: ForeignChainAddress::benchmark_value(),
		};
		let call = Call::<T>::ccm_deposit{
			source_asset: Asset::Usdc,
			deposit_amount: 1_000,
			destination_asset: Asset::Eth,
			destination_address: EncodedAddress::benchmark_value(),
			message_metadata: metadata,
		};
	}: {
		call.dispatch_bypass_filter(origin)?;
	}
	verify {
		assert_eq!(SwapQueue::<T>::get(), vec![Swap{
			swap_id: 1,
			from: Asset::Usdc,
			to: Asset::Eth,
			amount:(1_000 - 1),
			swap_type: SwapType::CcmPrincipal(1)
		},
		Swap{
			swap_id: 2,
			from: Asset::Usdc,
			to: Asset::Eth,
			amount:1,
			swap_type: SwapType::CcmGas(1)
		}]);
	}

	on_initialize {
		let a in 1..100;
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		T::AccountRoleRegistry::register_as_broker(&caller).unwrap();
		let origin = RawOrigin::Signed(caller);
		for i in 0..a {
			let call = Call::<T>::request_swap_deposit_address{
				source_asset: Asset::Usdc,
				destination_asset: Asset::Eth,
				destination_address: EncodedAddress::Eth(Default::default()),
				broker_commission_bps: Default::default(),
				message_metadata: None,
			};
			call.dispatch_bypass_filter(origin.clone().into())?;
		}
		let expiry = SwapTTL::<T>::get() + frame_system::Pallet::<T>::current_block_number();
		assert!(!SwapChannelExpiries::<T>::get(expiry).is_empty());
	}: {
		Pallet::<T>::on_initialize(expiry);
	} verify {
		assert!(SwapChannelExpiries::<T>::get(expiry).is_empty());
	}

	set_swap_ttl {
		let ttl = T::BlockNumber::from(1_000u32);
		let call = Call::<T>::set_swap_ttl {
			ttl
		};
	}: {
		let _ = call.dispatch_bypass_filter(<T as Chainflip>::EnsureGovernance::successful_origin());
	} verify {
		assert_eq!(crate::SwapTTL::<T>::get(), ttl);
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test,
	);
}
