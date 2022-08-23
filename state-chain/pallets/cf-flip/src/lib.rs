#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

mod imbalances;
mod on_charge_transaction;

pub mod weights;
use scale_info::TypeInfo;
pub use weights::WeightInfo;

use cf_traits::{Bonding, FeePayment, Slashing, StakeHandler, StakingInfo};
pub use imbalances::{Deficit, ImbalanceSource, InternalSource, Surplus};
pub use on_charge_transaction::FlipTransactionPayment;

use frame_support::{
	ensure,
	traits::{Get, Imbalance, OnKilledAccount, SignedImbalance},
};

use codec::{Decode, Encode, MaxEncodedLen};
use sp_runtime::{
	traits::{
		AtLeast32BitUnsigned, MaybeSerializeDeserialize, Saturating, UniqueSaturatedInto, Zero,
	},
	DispatchError, RuntimeDebug,
};
use sp_std::{fmt::Debug, marker::PhantomData, prelude::*};

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{StakeHandler, WaivedFees};
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// A 4-byte identifier for different reserves.
	pub type ReserveId = [u8; 4];

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// Implementation of EnsureOrigin trait for governance
		type EnsureGovernance: EnsureOrigin<Self::Origin>;

		/// The balance of an account.
		type Balance: Member
			+ Parameter
			+ MaxEncodedLen
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ Debug;

		/// The minimum amount required to keep an account open.
		#[pallet::constant]
		type ExistentialDeposit: Get<Self::Balance>;

		/// Blocks per day.
		#[pallet::constant]
		type BlocksPerDay: Get<Self::BlockNumber>;

		/// Providing updates on staking activity
		type StakeHandler: StakeHandler<ValidatorId = Self::AccountId, Amount = Self::Balance>;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;

		/// Handles the access of governance extrinsic
		type WaivedFees: WaivedFees<AccountId = Self::AccountId, Call = Self::Call>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Funds belonging to on-chain accounts.
	#[pallet::storage]
	#[pallet::getter(fn account)]
	pub type Account<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, FlipAccount<T::Balance>, ValueQuery>;

	/// Funds belonging to on-chain reserves.
	#[pallet::storage]
	#[pallet::getter(fn reserve)]
	pub type Reserve<T: Config> =
		StorageMap<_, Blake2_128Concat, ReserveId, T::Balance, ValueQuery>;

	/// The total number of tokens issued.
	#[pallet::storage]
	#[pallet::getter(fn total_issuance)]
	pub type TotalIssuance<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

	/// The slashing rate in percent - slash per day = n % of MBA.
	#[pallet::storage]
	#[pallet::getter(fn slashing_rate)]
	pub type SlashingRate<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

	/// The number of tokens currently off-chain.
	#[pallet::storage]
	#[pallet::getter(fn offchain_funds)]
	pub type OffchainFunds<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Some imbalance could not be settled and the remainder will be reverted. /[reverted_to,
		/// amount/]
		RemainingImbalance(ImbalanceSource<T::AccountId>, T::Balance),

		/// Slashing has been performed. /[account_id, amount/]
		SlashingPerformed(T::AccountId, T::Balance),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Not enough liquid funds.
		InsufficientLiquidity,
		/// Not enough reserves.
		InsufficientReserves,
		/// Invalid Slashing Rate: Has to be between 0 and 100
		InvalidSlashingRate,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the Slashing Rate. Slashing Rate is a percentage. It, therefore, has to be between 0
		/// and 100.
		///
		/// The dispatch origin of this function must be governance
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::error::BadOrigin)
		/// - [InvalidSlashingRate](Error::InvalidSlashingRate)
		///
		/// ## Dependencies
		///
		/// - [EnsureGovernance]
		#[pallet::weight(T::WeightInfo::set_slashing_rate())]
		pub fn set_slashing_rate(
			origin: OriginFor<T>,
			slashing_rate: T::Balance,
		) -> DispatchResultWithPostInfo {
			// Ensure the extrinsic was executed by the governance
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(slashing_rate <= T::Balance::from(100u32), Error::<T>::InvalidSlashingRate);
			// Set the slashing rate
			SlashingRate::<T>::set(slashing_rate);
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub total_issuance: T::Balance,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { total_issuance: Zero::zero() }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			TotalIssuance::<T>::set(self.total_issuance);
			OffchainFunds::<T>::set(self.total_issuance);
			SlashingRate::<T>::set(Default::default());
		}
	}
}

/// All balance information for a Flip account.
#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq, Default, RuntimeDebug)]
pub struct FlipAccount<Amount> {
	/// Amount that has been staked and is considered as a bid in the auction. Includes
	/// any bonded and vesting funds. Excludes any funds in the process of being claimed.
	stake: Amount,

	/// Amount that is bonded and cannot be withdrawn.
	bond: Amount,
}

impl<Balance: Saturating + Copy + Ord> FlipAccount<Balance> {
	/// The total balance excludes any funds that are in a pending claim request.
	pub fn total(&self) -> Balance {
		self.stake
	}

	/// Excludes the bond.
	pub fn liquid(&self) -> Balance {
		self.stake.saturating_sub(self.bond)
	}

	// The current bond
	pub fn bond(&self) -> Balance {
		self.bond
	}
}

/// Convenient alias for [SignedImbalance].
pub type FlipImbalance<T> = SignedImbalance<<T as Config>::Balance, Surplus<T>>;

impl<T: Config> From<Surplus<T>> for FlipImbalance<T> {
	fn from(surplus: Surplus<T>) -> Self {
		SignedImbalance::Positive(surplus)
	}
}

impl<T: Config> From<Deficit<T>> for FlipImbalance<T> {
	fn from(deficit: Deficit<T>) -> Self {
		SignedImbalance::Negative(deficit)
	}
}

impl<T: Config> Pallet<T> {
	/// The total number of tokens currently on-chain.
	pub fn onchain_funds() -> T::Balance {
		TotalIssuance::<T>::get() - OffchainFunds::<T>::get()
	}

	/// Total funds stored in an account.
	pub fn total_balance_of(account_id: &T::AccountId) -> T::Balance {
		Account::<T>::get(account_id).total()
	}

	/// Amount of funds allocated to a [Reserve].
	pub fn reserved_balance(reserve_id: ReserveId) -> T::Balance {
		Reserve::<T>::get(reserve_id)
	}

	/// Slashable funds for an account.
	pub fn slashable_funds(account_id: &T::AccountId) -> T::Balance {
		Account::<T>::get(account_id)
			.total()
			.saturating_sub(T::ExistentialDeposit::get())
	}

	/// Debits an account's staked balance.
	///
	/// *Warning:* Creates the flip account if it doesn't exist already, but *doesn't* ensure that
	/// the `System`-level account exists so should only be used with accounts that are known to
	/// exist.
	///
	/// Use [try_debit](Self::try_debit) instead when the existence of the account is unsure.
	///
	/// Debiting creates a surplus since we now have some funds that need to be allocated somewhere.
	pub fn debit(account_id: &T::AccountId, amount: T::Balance) -> Surplus<T> {
		Surplus::from_acct(account_id, amount)
	}

	/// Debits an account's staked balance, if the account exists and sufficient funds are
	/// available, otherwise returns `None`. Unlike [debit](Self::debit), does not create the
	/// account if it doesn't exist.
	pub fn try_debit(account_id: &T::AccountId, amount: T::Balance) -> Option<Surplus<T>> {
		Surplus::try_from_acct(account_id, amount, false)
	}

	/// Like `try_debit` but debits only the accounts liquid balance. Ensures that we don't burn
	/// more then the available liquid balance of the account.
	pub fn try_debit_from_liquid_funds(
		account_id: &T::AccountId,
		amount: T::Balance,
	) -> Option<Surplus<T>> {
		Surplus::try_from_acct(account_id, amount, true)
	}

	/// Credits an account with some staked funds. If the amount provided would result in overflow,
	/// does nothing.
	///
	/// Crediting an account creates a deficit since we need to take the credited funds from
	/// somewhere. In a sense we have spent money we don't have.
	pub fn credit(account_id: &T::AccountId, amount: T::Balance) -> Deficit<T> {
		Deficit::from_acct(account_id, amount)
	}

	/// Tries to settle an imbalance against an account. Returns `Ok(())` if the whole amount was
	/// settled, otherwise an `Err` containing any remaining imbalance.
	fn try_settle(
		account_id: &T::AccountId,
		imbalance: FlipImbalance<T>,
	) -> Result<(), FlipImbalance<T>> {
		match imbalance {
			SignedImbalance::Positive(surplus) => {
				let amount = surplus.peek();
				surplus
					.offset(Self::credit(account_id, amount))
					.same()
					.map(SignedImbalance::Positive)
					.unwrap_or_else(SignedImbalance::Negative)
			},
			SignedImbalance::Negative(deficit) => {
				let amount = deficit.peek();
				deficit
					.offset(Self::debit(account_id, amount))
					.same()
					.map(SignedImbalance::Negative)
					.unwrap_or_else(SignedImbalance::Positive)
			},
		}
		.drop_zero()
	}

	/// Settles an imbalance against an account. Any excess is reverted to source according to the
	/// rules defined in RevertImbalance.
	pub fn settle(account_id: &T::AccountId, imbalance: FlipImbalance<T>) {
		if let Err(remaining) = Self::try_settle(account_id, imbalance) {
			// Note `remaining` will be dropped and automatically reverted at the end of this
			// block.
			let (source, remainder) = match remaining {
				SignedImbalance::Positive(surplus) => (surplus.source.clone(), surplus.peek()),
				SignedImbalance::Negative(deficit) => (deficit.source.clone(), deficit.peek()),
			};
			Self::deposit_event(Event::<T>::RemainingImbalance(source, remainder));
		}
	}

	pub fn settle_imbalance<I: Into<FlipImbalance<T>>>(account_id: &T::AccountId, imbalance: I) {
		Self::settle(account_id, imbalance.into())
	}

	/// Decreases total issuance and returns a corresponding imbalance that must be reconciled.
	fn burn(amount: T::Balance) -> Deficit<T> {
		Deficit::from_burn(amount)
	}

	/// Increases total issuance and returns a corresponding imbalance that must be reconciled.
	fn mint(amount: T::Balance) -> Surplus<T> {
		Surplus::from_mint(amount)
	}

	/// Create some funds that have been added to the chain from outside.
	fn bridge_in(amount: T::Balance) -> Surplus<T> {
		Surplus::from_offchain(amount)
	}

	/// Send some funds off-chain.
	fn bridge_out(amount: T::Balance) -> Deficit<T> {
		Deficit::from_offchain(amount)
	}

	/// Withdraws *up to* `amount` from a reserve.
	///
	/// *Warning:* if the reserve does not exist, it will be created as a side effect.
	pub fn withdraw_reserves(reserve_id: ReserveId, amount: T::Balance) -> Surplus<T> {
		Surplus::from_reserve(reserve_id, amount)
	}

	/// Tries to withdraw funds from a reserve. Fails if the reserve doesn't exist or has
	/// insufficient funds.
	pub fn try_withdraw_reserves(
		reserve_id: ReserveId,
		amount: T::Balance,
	) -> Result<Surplus<T>, DispatchError> {
		Surplus::try_from_reserve(reserve_id, amount)
			.ok_or_else(|| Error::<T>::InsufficientReserves.into())
	}

	/// Deposit `amount` into the reserve identified by a `reserve_id`. Creates the reserve it it
	/// doesn't exist already.
	pub fn deposit_reserves(reserve_id: ReserveId, amount: T::Balance) -> Deficit<T> {
		Deficit::from_reserve(reserve_id, amount)
	}
}

impl<T: Config> StakingInfo for Pallet<T> {
	type AccountId = T::AccountId;
	type Balance = T::Balance;

	fn total_stake_of(account_id: &Self::AccountId) -> Self::Balance {
		Self::total_balance_of(account_id)
	}

	fn total_onchain_stake() -> Self::Balance {
		Self::onchain_funds()
	}
}

impl<T: Config> FeePayment for Pallet<T> {
	type Amount = T::Balance;
	type AccountId = T::AccountId;

	#[cfg(feature = "runtime-benchmarks")]
	fn mint_to_account(account_id: &Self::AccountId, amount: Self::Amount) {
		Pallet::<T>::settle(account_id, Pallet::<T>::mint(amount).into());
	}

	fn try_burn_fee(
		account_id: &Self::AccountId,
		amount: Self::Amount,
	) -> sp_runtime::DispatchResult {
		if let Some(surplus) = Pallet::<T>::try_debit_from_liquid_funds(account_id, amount) {
			let _ = surplus.offset(Pallet::<T>::burn(amount));
			Ok(())
		} else {
			Err(Error::<T>::InsufficientLiquidity.into())
		}
	}
}

pub struct Bonder<T>(PhantomData<T>);

impl<T: Config> Bonding for Bonder<T> {
	type ValidatorId = T::AccountId;
	type Amount = T::Balance;

	fn update_bond(authority: &Self::ValidatorId, bond: Self::Amount) {
		Account::<T>::mutate_exists(authority, |maybe_account| {
			if let Some(account) = maybe_account.as_mut() {
				account.bond = bond
			}
		})
	}
}

pub struct FlipIssuance<T>(PhantomData<T>);

impl<T: Config> cf_traits::Issuance for FlipIssuance<T> {
	type AccountId = T::AccountId;
	type Balance = T::Balance;
	type Surplus = Surplus<T>;

	fn mint(amount: Self::Balance) -> Surplus<T> {
		Pallet::<T>::mint(amount)
	}

	fn burn(amount: Self::Balance) -> Deficit<T> {
		Pallet::<T>::burn(amount)
	}

	fn total_issuance() -> Self::Balance {
		Pallet::<T>::total_issuance()
	}
}

impl<T: Config> cf_traits::StakeTransfer for Pallet<T> {
	type AccountId = T::AccountId;
	type Balance = T::Balance;
	type Handler = T::StakeHandler;

	fn locked_balance(account_id: &T::AccountId) -> Self::Balance {
		Account::<T>::get(account_id).bond()
	}

	fn staked_balance(account_id: &T::AccountId) -> Self::Balance {
		Account::<T>::get(account_id).total()
	}

	fn claimable_balance(account_id: &T::AccountId) -> Self::Balance {
		Account::<T>::get(account_id).liquid()
	}

	fn credit_stake(account_id: &Self::AccountId, amount: Self::Balance) -> Self::Balance {
		let incoming = Self::bridge_in(amount);
		Self::settle(account_id, SignedImbalance::Positive(incoming));
		T::StakeHandler::on_stake_updated(account_id, Self::staked_balance(account_id));
		Self::total_balance_of(account_id)
	}

	fn try_claim(account_id: &Self::AccountId, amount: Self::Balance) -> Result<(), DispatchError> {
		ensure!(
			amount <= Self::claimable_balance(account_id),
			DispatchError::from(Error::<T>::InsufficientLiquidity)
		);

		Self::settle(account_id, Self::bridge_out(amount).into());
		T::StakeHandler::on_stake_updated(account_id, Self::staked_balance(account_id));

		Ok(())
	}

	fn settle_claim(_amount: Self::Balance) {
		// Nothing to do.
	}

	fn revert_claim(account_id: &Self::AccountId, amount: Self::Balance) {
		Self::settle(account_id, Self::bridge_in(amount).into());
		T::StakeHandler::on_stake_updated(account_id, Self::staked_balance(account_id));
		// claim reverts automatically when dropped
	}
}

pub struct BurnFlipAccount<T: Config>(PhantomData<T>);

/// Implementation of `OnKilledAccount` ensures that we reconcile any flip dust remaining in the
/// account by burning it.
impl<T: Config> OnKilledAccount<T::AccountId> for BurnFlipAccount<T> {
	fn on_killed_account(account_id: &T::AccountId) {
		let dust = Pallet::<T>::total_balance_of(account_id);
		Pallet::<T>::settle(account_id, Pallet::<T>::burn(dust).into());
		Account::<T>::remove(account_id);
	}
}

pub struct FlipSlasher<T: Config>(PhantomData<T>);
/// An implementation of `Slashing` for Flip
impl<T, B> Slashing for FlipSlasher<T>
where
	T: Config<BlockNumber = B>,
	B: UniqueSaturatedInto<T::Balance>,
{
	type AccountId = T::AccountId;
	type BlockNumber = B;

	fn slash(account_id: &Self::AccountId, blocks_offline: Self::BlockNumber) {
		// Get the slashing rate
		let slashing_rate: T::Balance = SlashingRate::<T>::get();
		// Get the MAB aka the bond
		let bond = Account::<T>::get(account_id).bond;
		// Get blocks_offline as Balance
		let blocks_offline: T::Balance = blocks_offline.unique_saturated_into();
		// slash per day = n % of MAB
		let slash_per_day = (bond / T::Balance::from(100_u32)).saturating_mul(slashing_rate);
		// Burn per block
		let burn_per_block = slash_per_day / T::BlocksPerDay::get().unique_saturated_into();
		// Total amount of burn
		let total_burn = burn_per_block.saturating_mul(blocks_offline);
		// Burn the slashing fee
		Pallet::<T>::settle(account_id, Pallet::<T>::burn(total_burn).into());
		Pallet::<T>::deposit_event(Event::<T>::SlashingPerformed(account_id.clone(), total_burn));
	}
}
