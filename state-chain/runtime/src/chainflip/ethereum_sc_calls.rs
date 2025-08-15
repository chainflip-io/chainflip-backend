use crate::{chainflip::TypeInfo, Decode, Encode, Runtime, RuntimeCall};
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo},
	traits::UnfilteredDispatchable,
};
use pallet_cf_validator::Call;
use sp_runtime::traits::Dispatchable;

#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Debug, PartialOrd, Ord)]
pub enum DelegationApi {
	// todo: impl partial delegate/undelegate after the auction PR.
	Delegate {
		operator: <Runtime as frame_system::Config>::AccountId, /* Operator the amount to
		                                                         * delegate to */
	},
	Undelegate {},
	SetMaxBid {
		maybe_max_bid: Option<<Runtime as cf_traits::Chainflip>::Amount>,
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
					RuntimeCall::Validator(Call::<Runtime>::delegate { operator }).dispatch(origin),
				DelegationApi::Undelegate {} =>
					RuntimeCall::Validator(Call::<Runtime>::undelegate {}).dispatch(origin),
				DelegationApi::SetMaxBid { maybe_max_bid } =>
					RuntimeCall::Validator(Call::<Runtime>::set_max_bid { max_bid: maybe_max_bid })
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
					RuntimeCall::Validator(Call::<Runtime>::delegate { operator: operator.clone() })
						.get_dispatch_info(),
				DelegationApi::Undelegate {} =>
					RuntimeCall::Validator(Call::<Runtime>::undelegate {}).get_dispatch_info(),
				DelegationApi::SetMaxBid { maybe_max_bid } =>
					RuntimeCall::Validator(Call::<Runtime>::set_max_bid { max_bid: *maybe_max_bid })
						.get_dispatch_info(),
			},
		}
	}
}
