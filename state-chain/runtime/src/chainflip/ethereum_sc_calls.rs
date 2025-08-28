use crate::{chainflip::TypeInfo, Decode, Encode, EthereumAddress, Runtime, RuntimeCall};
use codec::MaxEncodedLen;
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo},
	traits::UnfilteredDispatchable,
};
use pallet_cf_funding::{Call as FundingCall, RedemptionAmount};
use pallet_cf_validator::{Call as ValidatorCall, DelegationAmount};
use sp_runtime::traits::Dispatchable;

#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Debug)]
pub enum DelegationApi {
	Delegate {
		operator: <Runtime as frame_system::Config>::AccountId,
		increase: DelegationAmount<<Runtime as cf_traits::Chainflip>::Amount>,
	},
	Undelegate {
		decrease: DelegationAmount<<Runtime as cf_traits::Chainflip>::Amount>,
	},
	Redeem {
		amount: RedemptionAmount<<Runtime as cf_traits::Chainflip>::Amount>,
		address: EthereumAddress,
		executor: Option<EthereumAddress>,
	},
}

#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Debug)]
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
				DelegationApi::Delegate { operator, increase } =>
					RuntimeCall::Validator(ValidatorCall::<Runtime>::delegate {
						operator,
						increase,
					})
					.dispatch(origin),
				DelegationApi::Undelegate { decrease } =>
					RuntimeCall::Validator(ValidatorCall::<Runtime>::undelegate { decrease })
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
				DelegationApi::Delegate { operator, increase } =>
					RuntimeCall::Validator(ValidatorCall::<Runtime>::delegate {
						operator: operator.clone(),
						increase: *increase,
					})
					.get_dispatch_info(),
				DelegationApi::Undelegate { decrease } =>
					RuntimeCall::Validator(ValidatorCall::<Runtime>::undelegate {
						decrease: *decrease,
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
