use super::*;
use frame_support::{sp_runtime::traits::Zero, DefaultNoBound};
use sp_std::marker::PhantomData;

pub trait GetAmount<A> {
	fn amount(&self) -> A;
}

impl<A: AtLeast32BitUnsigned + Copy> GetAmount<A> for A {
	fn amount(&self) -> A {
		*self
	}
}

pub trait DepositTracker<C: Chain> {
	fn total(&self) -> C::ChainAmount;

	fn register_deposit(
		&mut self,
		amount: C::ChainAmount,
		deposit_details: &C::DepositDetails,
		deposit_channel: &C::DepositChannel,
	);

	fn withdraw_all(
		&mut self,
		tracked_data: &C::TrackedData,
	) -> (Vec<C::FetchParams>, C::ChainAmount);

	/// Some(vec![]) means that we don't need to fetch anything - we already have enough fetched
	/// funds to cover the withdrawal. `None` means that we can't cover the withdrawal. In this
	/// case, the update to the deposit tracker should *not* be persisted to storage.
	///
	/// The returned amount is the total net spendable amount, ie. fees are already deducted.
	fn withdraw_at_least(
		&mut self,
		amount: <C as Chain>::ChainAmount,
		tracked_data: &C::TrackedData,
	) -> Option<(Vec<C::FetchParams>, <C as Chain>::ChainAmount)>;

	/// Called when a channel is closed.
	///
	/// Returns Some(_) if the address can be re-used, otherwise None.
	fn maybe_recycle_channel(&mut self, _channel: C::DepositChannel) -> Option<C::DepositChannel> {
		None
	}

	/// Called when a fetch is completed.
	fn on_fetch_completed(&mut self, _channel: &C::DepositChannel) {}
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
		_: &C::DepositChannel,
	) {
	}

	fn withdraw_all(
		&mut self,
		_: &C::TrackedData,
	) -> (Vec<C::FetchParams>, <C as Chain>::ChainAmount) {
		(vec![], self.total())
	}

	fn withdraw_at_least(
		&mut self,
		_: C::ChainAmount,
		_: &C::TrackedData,
	) -> Option<(Vec<C::FetchParams>, <C as Chain>::ChainAmount)> {
		None
	}
}
