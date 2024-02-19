#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_chains::{address::EncodedAddress, benchmarking_value::BenchmarkValue};
use cf_primitives::FLIPPERINOS_PER_FLIP;
use cf_traits::{AccountRoleRegistry, FeePayment};
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{OnNewAccount, UnfilteredDispatchable},
};
use frame_system::RawOrigin;

#[benchmarks(
	where <T::FeePayment as cf_traits::FeePayment>::Amount: From<u128>
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn request_swap_deposit_address() {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		assert_ok!(T::AccountRoleRegistry::register_as_broker(&caller));
		// A non-zero balance is required to pay for the channel opening fee.
		T::FeePayment::mint_to_account(&caller, (5 * FLIPPERINOS_PER_FLIP).into());

		let origin = RawOrigin::Signed(caller);
		let call = Call::<T>::request_swap_deposit_address {
			source_asset: Asset::Eth,
			destination_asset: Asset::Usdc,
			destination_address: EncodedAddress::benchmark_value(),
			broker_commission_bps: 0,
			boost_fee: 0,
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
			SwapQueue::<T>::get(
				<frame_system::Pallet<T>>::block_number() + SWAP_DELAY_BLOCKS.into()
			),
			vec![Swap::new(
				1,
				Asset::Usdc,
				Asset::Eth,
				deposit_amount,
				SwapType::Swap(ForeignChainAddress::benchmark_value()),
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
			SwapQueue::<T>::get(
				<frame_system::Pallet<T>>::block_number() + SWAP_DELAY_BLOCKS.into()
			),
			vec![
				Swap::new(1, Asset::Usdc, Asset::Eth, 1_000 - 1, SwapType::CcmPrincipal(1),),
				Swap::new(2, Asset::Usdc, Asset::Eth, 1, SwapType::CcmGas(1),)
			]
		);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
