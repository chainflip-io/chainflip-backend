use crate::{chainflip::TypeInfo, Decode, Encode, EthereumAddress, Runtime, RuntimeCall};
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo},
	traits::UnfilteredDispatchable,
};
use pallet_cf_funding::{Call as FundingCall, RedemptionAmount};
use pallet_cf_validator::Call as ValidatorCall;
use sp_runtime::traits::Dispatchable;

#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Debug, PartialOrd, Ord)]
pub enum DelegationApi {
	// todo: impl partial delegate/undelegate after the auction PR.
	Delegate {
		operator: <Runtime as frame_system::Config>::AccountId, /* Operator the amount to
		                                                         * delegate to */
	},
	Undelegate,
	SetMaxBid {
		maybe_max_bid: Option<<Runtime as cf_traits::Chainflip>::Amount>,
	},
	Redeem {
		amount: RedemptionAmount<<Runtime as cf_traits::Chainflip>::Amount>,
		address: EthereumAddress,
		executor: Option<EthereumAddress>,
	},
}

#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Debug, PartialOrd, Ord)]
pub enum EthereumSCApi {
	Delegation(DelegationApi),
	// reserved for future Apis for example Loan(LoanApi)...
	// This allows us to update the API without breaking the encoding.
}

impl UnfilteredDispatchable for EthereumSCApi {
	type RuntimeOrigin = <Runtime as frame_system::Config>::RuntimeOrigin;
	fn dispatch_bypass_filter(
		self,
		origin: Self::RuntimeOrigin,
	) -> frame_support::dispatch::DispatchResultWithPostInfo {
		match self {
			EthereumSCApi::Delegation(delegation_api) => match delegation_api {
				DelegationApi::Delegate { operator } =>
					RuntimeCall::Validator(ValidatorCall::<Runtime>::delegate { operator, max_bid: None })
						.dispatch(origin),
				DelegationApi::Undelegate =>
					RuntimeCall::Validator(ValidatorCall::<Runtime>::undelegate { decrement: None }).dispatch(origin),
				DelegationApi::SetMaxBid { maybe_max_bid } =>
					RuntimeCall::Validator(ValidatorCall::<Runtime>::set_max_bid {
						max_bid: maybe_max_bid,
					})
					.dispatch(origin),
				DelegationApi::Redeem { amount, address, executor } =>
					RuntimeCall::Funding(FundingCall::<Runtime>::redeem {
						amount,
						address,
						executor,
					})
					.dispatch(origin),
			},
		}
	}
}

impl GetDispatchInfo for EthereumSCApi {
	fn get_dispatch_info(&self) -> DispatchInfo {
		match self {
			EthereumSCApi::Delegation(delegation_api) => match delegation_api {
				DelegationApi::Delegate { operator } =>
					RuntimeCall::Validator(ValidatorCall::<Runtime>::delegate {
						operator: operator.clone(),
						max_bid: None,
					})
					.get_dispatch_info(),
				DelegationApi::Undelegate {} =>
					RuntimeCall::Validator(ValidatorCall::<Runtime>::undelegate { decrement: None })
						.get_dispatch_info(),
				DelegationApi::SetMaxBid { maybe_max_bid } =>
					RuntimeCall::Validator(ValidatorCall::<Runtime>::set_max_bid {
						max_bid: *maybe_max_bid,
					})
					.get_dispatch_info(),
				DelegationApi::Redeem { amount, address, executor } =>
					RuntimeCall::Funding(FundingCall::<Runtime>::redeem {
						amount: *amount,
						address: *address,
						executor: *executor,
					})
					.get_dispatch_info(),
			},
		}
	}
}
