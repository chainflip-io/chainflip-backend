#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_chains::{
	address::EncodedAddress, benchmarking_value::BenchmarkValue, eth::Address as EthereumAddress,
	evm::U256,
};
use cf_primitives::{AccountRole, AffiliateShortId, Beneficiary, FLIPPERINOS_PER_FLIP};
use cf_traits::{AccountRoleRegistry, Chainflip, FeePayment};
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{OnNewAccount, OriginTrait, UnfilteredDispatchable},
};
use frame_system::RawOrigin;

#[allow(clippy::multiple_bound_locations)]
#[benchmarks(
	where <T::FeePayment as cf_traits::FeePayment>::Amount: From<u128>
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn request_swap_deposit_address() {
		let caller = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Broker,
		)
		.unwrap();

		// A non-zero balance is required to pay for the channel opening fee.
		T::FeePayment::mint_to_account(&caller, (5 * FLIPPERINOS_PER_FLIP).into());

		let origin = RawOrigin::Signed(caller.clone());
		let call = Call::<T>::request_swap_deposit_address {
			source_asset: Asset::Eth,
			destination_asset: Asset::Usdc,
			destination_address: EncodedAddress::benchmark_value(),
			broker_commission: 10,
			boost_fee: 0,
			channel_metadata: None,
			refund_parameters: ChannelRefundParametersEncoded {
				retry_duration: 100,
				refund_address: EncodedAddress::benchmark_value(),
				min_price: U256::from(0),
			},
		};

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin.into()));
		}
	}

	#[benchmark]
	fn request_swap_deposit_address_with_affiliates() {
		let caller = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Broker,
		)
		.unwrap();

		// A non-zero balance is required to pay for the channel opening fee.
		T::FeePayment::mint_to_account(&caller, (5 * FLIPPERINOS_PER_FLIP).into());

		let affiliate_fees = (0..4)
			.map(|i| {
				let account = frame_benchmarking::account::<T::AccountId>("beneficiary", i, 0);
				frame_benchmarking::whitelist_account!(account);
				frame_system::Pallet::<T>::inc_providers(&account);
				<T as frame_system::Config>::OnNewAccount::on_new_account(&account);
				<<T as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<T>>::register_as_broker(&account).unwrap();
				Beneficiary { account, bps: 10 }
			})
			.collect::<Vec<_>>()
			.try_into()
			.unwrap();

		let origin = RawOrigin::Signed(caller.clone());
		let call = Call::<T>::request_swap_deposit_address_with_affiliates {
			source_asset: Asset::Eth,
			destination_asset: Asset::Usdc,
			destination_address: EncodedAddress::benchmark_value(),
			broker_commission: 10,
			boost_fee: 0,
			channel_metadata: None,
			refund_parameters: ChannelRefundParametersEncoded {
				retry_duration: 100,
				refund_address: EncodedAddress::benchmark_value(),
				min_price: U256::from(0),
			},
			affiliate_fees,
			dca_parameters: None,
		};

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin.into()));
		}
	}

	#[benchmark]
	fn withdraw() {
		let caller = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Broker,
		)
		.unwrap();

		T::BalanceApi::credit_account(&caller, Asset::Eth, 200);

		#[extrinsic_call]
		withdraw(RawOrigin::Signed(caller.clone()), Asset::Eth, EncodedAddress::benchmark_value());
	}

	#[benchmark]
	fn register_as_broker() {
		let caller: T::AccountId = whitelisted_caller();
		frame_system::Pallet::<T>::inc_providers(&caller);
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);

		#[extrinsic_call]
		register_as_broker(RawOrigin::Signed(caller.clone()));

		T::AccountRoleRegistry::ensure_broker(RawOrigin::Signed(caller).into())
			.expect("Caller should be registered as broker");
	}

	#[benchmark]
	fn deregister_as_broker() {
		let caller = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Broker,
		)
		.unwrap();

		#[extrinsic_call]
		deregister_as_broker(RawOrigin::Signed(caller.clone()));

		T::AccountRoleRegistry::ensure_broker(RawOrigin::Signed(caller).into())
			.expect_err("Caller should no longer be registered as broker");
	}

	#[benchmark]
	fn open_private_btc_channel() {
		let broker_id =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Broker).unwrap();

		let caller = OriginFor::<T>::signed(broker_id.clone());

		T::FeePayment::mint_to_account(&broker_id, (200 * FLIPPERINOS_PER_FLIP).into());

		#[block]
		{
			assert_ok!(Pallet::<T>::open_private_btc_channel(caller));
		}

		assert!(
			BrokerPrivateBtcChannels::<T>::contains_key(&broker_id),
			"Private channel must have been opened"
		);
	}

	#[benchmark]
	fn close_private_btc_channel() {
		let broker_id =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Broker).unwrap();

		let caller = OriginFor::<T>::signed(broker_id.clone());

		T::FeePayment::mint_to_account(&broker_id, (200 * FLIPPERINOS_PER_FLIP).into());

		assert_ok!(Pallet::<T>::open_private_btc_channel(caller.clone()));

		assert!(
			BrokerPrivateBtcChannels::<T>::contains_key(&broker_id),
			"Private channel must have been opened"
		);

		#[block]
		{
			assert_ok!(Pallet::<T>::close_private_btc_channel(caller));
		}

		assert!(
			!BrokerPrivateBtcChannels::<T>::contains_key(&broker_id),
			"Private channel must have been closed"
		);
	}

	#[benchmark]
	fn register_affiliate() {
		let broker_id =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Broker).unwrap();

		const IDX: u8 = 0;
		let caller = OriginFor::<T>::signed(broker_id.clone());

		let withdrawal_address: EthereumAddress = Default::default();

		#[block]
		{
			assert_ok!(Pallet::<T>::register_affiliate(caller.clone(), withdrawal_address,));
		}

		assert!(
			AffiliateIdMapping::<T>::get(&broker_id, AffiliateShortId::from(IDX)).is_some(),
			"Affiliate must have been registered"
		);
	}

	#[benchmark]
	fn affiliate_withdrawal_request() {
		let broker_id =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Broker).unwrap();

		const IDX: u8 = 0;
		let caller = OriginFor::<T>::signed(broker_id.clone());
		let withdrawal_address: EthereumAddress = Default::default();

		assert_ok!(Pallet::<T>::register_affiliate(caller.clone(), withdrawal_address,));

		let affiliate_account_id =
			AffiliateIdMapping::<T>::get(&broker_id, AffiliateShortId::from(IDX))
				.expect("Affiliate must be registered!");

		T::BalanceApi::credit_account(&affiliate_account_id, Asset::Usdc, 200);

		#[block]
		{
			assert_ok!(Pallet::<T>::affiliate_withdrawal_request(
				caller.clone(),
				affiliate_account_id.clone()
			));
		}

		assert_eq!(
			T::BalanceApi::get_balance(&affiliate_account_id, Asset::Usdc),
			0,
			"Expect account balance to be 0 after distribution."
		);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
