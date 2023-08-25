use super::*;
use frame_support::{
	sp_runtime::traits::{Saturating, Zero},
	DefaultNoBound,
};
use sp_std::marker::PhantomData;

pub trait DepositTracker<C: Chain> {
	fn total(&self) -> C::ChainAmount;
	fn register_deposit(
		&mut self,
		amount: C::ChainAmount,
		deposit_details: &C::DepositDetails,
		deposit_channel: &DepositChannel<C>,
	);
	fn register_transfer(&mut self, amount: C::ChainAmount);
	fn mark_as_fetched(&mut self, amount: C::ChainAmount);
}

#[derive(
	CloneNoBound,
	DefaultNoBound,
	RuntimeDebug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
)]
#[scale_info(skip_type_params(C))]
pub struct SimpleDepositTracker<C: Chain> {
	pub unfetched: C::ChainAmount,
	pub fetched: C::ChainAmount,
}

impl<C: Chain> DepositTracker<C> for SimpleDepositTracker<C> {
	fn total(&self) -> C::ChainAmount {
		self.unfetched.saturating_add(self.fetched)
	}

	fn register_deposit(
		&mut self,
		amount: C::ChainAmount,
		_: &C::DepositDetails,
		_: &DepositChannel<C>,
	) {
		self.unfetched.saturating_accrue(amount);
	}

	fn register_transfer(&mut self, amount: C::ChainAmount) {
		self.fetched.saturating_reduce(amount);
	}

	fn mark_as_fetched(&mut self, amount: C::ChainAmount) {
		let amount = amount.min(self.unfetched);
		self.unfetched -= amount;
		self.fetched.saturating_accrue(amount);
	}
}

#[derive(
	CloneNoBound,
	DefaultNoBound,
	RuntimeDebug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct NoDepositTracking<C: Chain>(PhantomData<C>);

impl<C: Chain> DepositTracker<C> for NoDepositTracking<C> {
	fn total(&self) -> C::ChainAmount {
		Zero::zero()
	}

	fn register_deposit(
		&mut self,
		_: C::ChainAmount,
		_: &C::DepositDetails,
		_: &DepositChannel<C>,
	) {
	}

	fn register_transfer(&mut self, _: C::ChainAmount) {}

	fn mark_as_fetched(&mut self, _: C::ChainAmount) {}
}
