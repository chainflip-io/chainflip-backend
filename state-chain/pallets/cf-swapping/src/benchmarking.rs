//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_chains::{address::EncodedAddress, benchmarking_value::BenchmarkValue};
use cf_primitives::AccountRole;
use cf_traits::AccountRoleRegistry;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::dispatch::UnfilteredDispatchable;
use frame_system::RawOrigin;

fn generate_swaps<T: Config>(amount: u32, from: Asset, to: Asset) -> Vec<Swap> {
	let mut swaps: Vec<Swap> = vec![];
	for i in 1..amount {
		swaps.push(Swap {
			swap_id: i as u64,
			from,
			to,
			amount: 3,
			swap_type: SwapType::Swap(ForeignChainAddress::Eth(Default::default())),
		});
	}
	swaps
}

benchmarks! {
	register_swap_intent {
		let caller: T::AccountId = whitelisted_caller();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::Relayer);
		let origin = RawOrigin::Signed(caller);
		let call = Call::<T>::register_swap_intent {
			ingress_asset: Asset::Eth,
			egress_asset: Asset::Usdc,
			egress_address: EncodedAddress::benchmark_value(),
			relayer_commission_bps: 0,
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
		EarnedRelayerFees::<T>::insert(caller.clone(), Asset::Eth, 200);
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::Relayer);
	} : _(
		RawOrigin::Signed(caller.clone()),
		Asset::Eth,
		EncodedAddress::benchmark_value()
	)

	schedule_swap_by_witnesser {
		let origin = T::EnsureWitnessed::successful_origin();
		let call = Call::<T>::schedule_swap_by_witnesser{
			from: Asset::Usdc,
			to: Asset::Eth,
			ingress_amount: 1_000,
			egress_address: EncodedAddress::benchmark_value()
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
			swap_type: SwapType::Swap(ForeignChainAddress::Eth(Default::default()))
		}]);
	}

	ccm_ingress {
		let origin = T::EnsureWitnessed::successful_origin();
		let metadata = CcmIngressMetadata {
			message: vec![0x00],
			gas_budget: 1,
			refund_address: ForeignChainAddress::Eth(Default::default()),
			source_address: ForeignChainAddress::Eth(Default::default())
		};
		let call = Call::<T>::ccm_ingress{
			ingress_asset: Asset::Usdc,
			ingress_amount: 1_000,
			egress_asset: Asset::Eth,
			egress_address: EncodedAddress::benchmark_value(),
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

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test,
	);
}
