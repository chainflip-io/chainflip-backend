use crate::{chainflip::TypeInfo, Decode, Encode, EthereumAddress, Runtime, RuntimeCall};
use cf_primitives::FlipBalance;
use codec::MaxEncodedLen;
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo},
	traits::UnfilteredDispatchable,
	Deserialize, Serialize,
};
use pallet_cf_funding::{Call as FundingCall, RedemptionAmount};
use pallet_cf_validator::Call as ValidatorCall;
use sp_runtime::{traits::Dispatchable, AccountId32};

// Re-export here because it's need to construct the DelegationApi enum.
pub use pallet_cf_validator::DelegationAmount;

pub struct EthereumAccount(pub EthereumAddress);

impl EthereumAccount {
	pub fn into_account_id(&self) -> <Runtime as frame_system::Config>::AccountId {
		let mut data = [0u8; 32];
		data[12..32].copy_from_slice(&self.0 .0);
		AccountId32::new(data)
	}
}

#[derive(
	Clone, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Debug, Serialize, Deserialize,
)]
pub enum DelegationApi<A> {
	Delegate {
		operator: <Runtime as frame_system::Config>::AccountId,
		increase: DelegationAmount<A>,
	},
	Undelegate {
		decrease: DelegationAmount<A>,
	},
	Redeem {
		amount: RedemptionAmount<A>,
		address: EthereumAddress,
		executor: Option<EthereumAddress>,
	},
}

impl<A> DelegationApi<A> {
	pub fn try_fmap<B, E>(self, f: impl FnOnce(A) -> Result<B, E>) -> Result<DelegationApi<B>, E> {
		match self {
			DelegationApi::Delegate { operator, increase } =>
				Ok(DelegationApi::Delegate { operator, increase: increase.try_fmap(f)? }),
			DelegationApi::Undelegate { decrease } =>
				Ok(DelegationApi::Undelegate { decrease: decrease.try_fmap(f)? }),
			DelegationApi::Redeem { amount, address, executor } =>
				Ok(DelegationApi::Redeem { amount: amount.try_fmap(f)?, address, executor }),
		}
	}
}

#[derive(
	Clone, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Debug, Serialize, Deserialize,
)]
#[serde(tag = "API")]
pub enum EthereumSCApi<A> {
	Delegation { call: DelegationApi<A> },
	// reserved for future Apis for example Loan(LoanApi)...
	// This allows us to update the API without breaking the encoding.
}

impl<A> EthereumSCApi<A> {
	pub fn try_fmap<B, E>(self, f: impl FnOnce(A) -> Result<B, E>) -> Result<EthereumSCApi<B>, E> {
		match self {
			EthereumSCApi::Delegation { call } =>
				Ok(EthereumSCApi::Delegation { call: call.try_fmap(f)? }),
		}
	}
}

impl UnfilteredDispatchable for EthereumSCApi<FlipBalance> {
	type RuntimeOrigin = <Runtime as frame_system::Config>::RuntimeOrigin;
	fn dispatch_bypass_filter(
		self,
		origin: Self::RuntimeOrigin,
	) -> frame_support::dispatch::DispatchResultWithPostInfo {
		match self {
			EthereumSCApi::Delegation { call } => match call {
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

impl GetDispatchInfo for EthereumSCApi<FlipBalance> {
	fn get_dispatch_info(&self) -> DispatchInfo {
		match self {
			EthereumSCApi::Delegation { call } => match call {
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
