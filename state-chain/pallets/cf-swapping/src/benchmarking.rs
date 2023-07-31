//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_chains::{address::EncodedAddress, benchmarking_value::BenchmarkValue};
use cf_traits::{AccountRoleRegistry, Chainflip};
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::{dispatch::UnfilteredDispatchable, traits::OnNewAccount};
use frame_system::RawOrigin;

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
			deposit_metadata: None,
		};
	} : { call.dispatch_bypass_filter(origin.into())?; }

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

	schedule_swap_from_contract {
		let deposit_amount = 1_000;

		// reduce minimum swap amount
		let gov_origin = T::EnsureGovernance::successful_origin();
		let call = Call::<T>::set_minimum_swap_amount{
			asset: Asset::Usdc,
			amount: 1u128,
		};
		call.dispatch_bypass_filter(gov_origin)?;

		let witness_origin = T::EnsureWitnessed::successful_origin();
		let call = Call::<T>::schedule_swap_from_contract{
			from: Asset::Usdc,
			to: Asset::Eth,
			deposit_amount,
			destination_address: EncodedAddress::benchmark_value(),
			tx_hash: [0; 32],
		};
	}: {
		call.dispatch_bypass_filter(witness_origin)?;
	}
	verify {
		assert_eq!(SwapQueue::<T>::get(), vec![Swap::new(
			1,
			Asset::Usdc,
			Asset::Eth,
			deposit_amount,
			SwapType::Swap(ForeignChainAddress::benchmark_value())
		)]);
	}

	ccm_deposit {
		// reduce minimum swap amount
		let gov_origin = T::EnsureGovernance::successful_origin();
		let call = Call::<T>::set_minimum_swap_amount{
			asset: Asset::Usdc,
			amount: 1u128,
		};
		call.dispatch_bypass_filter(gov_origin)?;

		let origin = T::EnsureWitnessed::successful_origin();
		let metadata = CcmDepositMetadata {
			source_chain: ForeignChain::Ethereum,
			source_address: Some(ForeignChainAddress::benchmark_value()),
			channel_metadata: CcmChannelMetadata {
				message: vec![0x00],
				gas_budget: 1,
				cf_parameters: vec![],
			}
		};
		let call = Call::<T>::ccm_deposit{
			source_asset: Asset::Usdc,
			deposit_amount: 1_000,
			destination_asset: Asset::Eth,
			destination_address: EncodedAddress::benchmark_value(),
			deposit_metadata: metadata,
			tx_hash: Default::default(),
		};
	}: {
		call.dispatch_bypass_filter(origin)?;
	}
	verify {
		assert_eq!(SwapQueue::<T>::get(), vec![Swap::new(
			1,
			Asset::Usdc,
			Asset::Eth,
			1_000 - 1,
			SwapType::CcmPrincipal(1)
		),
		Swap::new(
			2,
			Asset::Usdc,
			Asset::Eth,
			1,
			SwapType::CcmGas(1)
		)]);
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
				deposit_metadata: None,
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

	set_minimum_swap_amount {
		let asset = Asset::Eth;
		let amount = 1_000;
		let call = Call::<T>::set_minimum_swap_amount {
			asset,
			amount,
		};
	}: {
		let _ = call.dispatch_bypass_filter(<T as Chainflip>::EnsureGovernance::successful_origin());
	} verify {
		assert_eq!(crate::MinimumSwapAmount::<T>::get(asset), amount);
	}

	set_minimum_ccm_gas_budget {
		let asset = Asset::Eth;
		let amount = 1_000;
		let call = Call::<T>::set_minimum_ccm_gas_budget {
			asset,
			amount,
		};
	}: {
		let _ = call.dispatch_bypass_filter(<T as Chainflip>::EnsureGovernance::successful_origin());
	} verify {
		assert_eq!(crate::MinimumCcmGasBudget::<T>::get(asset), amount);
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test,
	);
}
