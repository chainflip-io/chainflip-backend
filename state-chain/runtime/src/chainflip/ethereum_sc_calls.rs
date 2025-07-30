use crate::{chainflip::TypeInfo, Decode, Encode, EthereumAddress, Runtime};
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo},
	traits::UnfilteredDispatchable,
};

#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Debug, PartialOrd, Ord)]
pub enum DelegationApi {
	Delegate {
		delegator: EthereumAddress, // Ethereum Address of the delegator
		operator: <Runtime as frame_system::Config>::AccountId, // Operator the amount to delegate to
	},
	Undelegate {
		delegator: EthereumAddress, // Ethereum Address of the delegator
		operator: <Runtime as frame_system::Config>::AccountId, // Operator the amount was delegated to
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
		_origin: Self::RuntimeOrigin,
	) -> frame_support::dispatch::DispatchResultWithPostInfo {
		match self {
			EthereumSCApi::Delegation(delegation_api) => match delegation_api {
				DelegationApi::Delegate { delegator: _, operator: _ } => todo!(),
				DelegationApi::Undelegate { delegator: _, operator: _ } => todo!(),
			},
		}
	}
}

impl GetDispatchInfo for EthereumSCApi {
	fn get_dispatch_info(&self) -> DispatchInfo {
		todo!()
	}
}
