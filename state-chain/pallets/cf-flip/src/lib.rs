#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

mod mock;
mod tests;

mod benchmarking;

mod imbalances;
mod on_charge_transaction;

pub mod weights;
use cf_primitives::FlipBalance;
use scale_info::TypeInfo;
pub use weights::WeightInfo;

use cf_traits::{AccountInfo, Bonding, FeePayment, FundingInfo, OnAccountFunded, Slashing};
pub use imbalances::{Deficit, ImbalanceSource, InternalSource, Surplus};
pub use on_charge_transaction::FlipTransactionPayment;

use frame_support::{
	ensure,
	traits::{Get, Imbalance, OnKilledAccount, SignedImbalance},
};

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{
		traits::{AtLeast32BitUnsigned, MaybeSerializeDeserialize, Saturating, Zero},
		DispatchError, Permill, RuntimeDebug,
	},
};
use frame_system::pallet_prelude::*;
use sp_std::{fmt::Debug, marker::PhantomData, prelude::*};

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{Chainflip, OnAccountFunded, WaivedFees};

	/// A 4-byte identifier for different reserves.
	pub type ReserveId = [u8; 4];

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip<Amount = Self::Balance> {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The balance of an account.
		type Balance: Member
			+ Parameter
			+ MaxEncodedLen
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ Debug
			+ From<u128>;

		/// Blocks per day.
		#[pallet::constant]
		type BlocksPerDay: Get<BlockNumberFor<Self>>;

		/// Providing updates on funding activity
		type OnAccountFunded: OnAccountFunded<ValidatorId = Self::AccountId, Amount = Self::Balance>;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;

		/// Handles the access of governance extrinsic
		type WaivedFees: WaivedFees<
			AccountId = Self::AccountId,
			RuntimeCall = <Self as frame_system::Config>::RuntimeCall,
		>;
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

	#[pallet::storage]
	#[pallet::getter(fn pending_redemptions_reserve)]
	pub type PendingRedemptionsReserve<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, T::Balance>;

	/// The total number of tokens issued.
	#[pallet::storage]
	#[pallet::getter(fn total_issuance)]
	pub type TotalIssuance<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

	/// The per-day slashing rate expressed as a proportion of a validator's bond.
	#[pallet::storage]
	#[pallet::getter(fn slashing_rate)]
	pub type SlashingRate<T: Config> = StorageValue<_, Permill, ValueQuery>;

	/// The number of tokens currently off-chain.
	#[pallet::storage]
	#[pallet::getter(fn offchain_funds)]
	pub type OffchainFunds<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Some imbalance could not be settled and the remainder will be reverted.
		RemainingImbalance {
			who: ImbalanceSource<T::AccountId>,
			remaining_imbalance: T::Balance,
		},
		SlashingPerformed {
			who: T::AccountId,
			amount: T::Balance,
		},
		AccountReaped {
			who: T::AccountId,
			dust_burned: T::Balance,
		},
		SlashingRateUpdated {
			slashing_rate: Permill,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Not enough liquid funds.
		InsufficientLiquidity,
		/// Not enough reserves.
		InsufficientReserves,
		/// No pending redemption for this ID.
		NoPendingRedemptionForThisID,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the PER BLOCK slashing rate.
		///
		/// The dispatch origin of this function must be governance
		///
		/// ## Events
		///
		/// - [SlashingRateUpdated]
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::error::BadOrigin)
		///
		/// ## Dependencies
		///
		/// - [EnsureGovernance]
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::set_slashing_rate())]
		pub fn set_slashing_rate(origin: OriginFor<T>, slashing_rate: Permill) -> DispatchResult {
			// Ensure the extrinsic was executed by the governance
			T::EnsureGovernance::ensure_origin(origin)?;
			// Set the slashing rate
			SlashingRate::<T>::set(slashing_rate);
			Self::deposit_event(Event::SlashingRateUpdated { slashing_rate });
			Ok(())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub total_issuance: T::Balance,
		pub daily_slashing_rate: Permill,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			use frame_support::sp_runtime::traits::Zero;
			Self { total_issuance: Zero::zero(), daily_slashing_rate: Permill::zero() }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			TotalIssuance::<T>::set(self.total_issuance);
			OffchainFunds::<T>::set(self.total_issuance);
			SlashingRate::<T>::set(self.daily_slashing_rate);
		}
	}
}

/// All balance information for a Flip account.
#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq, Default, RuntimeDebug)]
pub struct FlipAccount<Amount> {
	/// Total amount of funds in account. Includes any bonded and vesting funds. Excludes any funds
	/// in the process of being redeemed.
	balance: Amount,

	/// Amount that is bonded and cannot be withdrawn.
	bond: Amount,
}

impl<Balance: AtLeast32BitUnsigned + Copy> FlipAccount<Balance> {
	/// The total balance excludes any funds that are in a pending redemption request.
	pub fn total(&self) -> Balance {
		self.balance
	}

	/// Excludes the bond.
	pub fn liquid(&self) -> Balance {
		self.balance.saturating_sub(self.bond)
	}

	/// The current bond
	pub fn bond(&self) -> Balance {
		self.bond
	}

	/// Account can only be slashed if its balance is higher than 20% of the bond.
	pub fn can_be_slashed(&self, slash_amount: Balance) -> bool {
		self.balance.saturating_sub(slash_amount) > self.bond / 5u32.into()
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

	/// Debits an account balance.
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

	/// Debits an account balance, if the account exists and sufficient funds are
	/// available, otherwise returns `None`. Unlike [debit](Self::debit), does not create the
	/// account if it doesn't exist.
	pub fn try_debit(account_id: &T::AccountId, amount: T::Balance) -> Option<Surplus<T>> {
		Surplus::try_from_acct(account_id, amount, false)
	}

	/// Like `try_debit` but debits only the accounts liquid balance. Ensures that we don't burn
	/// more than the available liquidity of the account and never touch the bonded balance.
	pub fn try_debit_from_liquid_funds(
		account_id: &T::AccountId,
		amount: T::Balance,
	) -> Option<Surplus<T>> {
		Surplus::try_from_acct(account_id, amount, true)
	}

	/// Credits an account with some funds. If the amount provided would result in overflow,
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
			Self::deposit_event(Event::<T>::RemainingImbalance {
				who: source,
				remaining_imbalance: remainder,
			});
		}
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

	/// Tries to withdraw funds from a pending redemption. Fails if the redemption doesn't exist
	pub fn try_withdraw_pending_redemption(
		account_id: &T::AccountId,
	) -> Result<Surplus<T>, DispatchError> {
		Surplus::try_from_pending_redemptions_reserve(account_id)
			.ok_or_else(|| Error::<T>::NoPendingRedemptionForThisID.into())
	}

	/// Deposit `amount` into the reserve identified by a `reserve_id`. Creates the reserve it it
	/// doesn't exist already.
	pub fn deposit_reserves(reserve_id: ReserveId, amount: T::Balance) -> Deficit<T> {
		Deficit::from_reserve(reserve_id, amount)
	}

	/// Create a pending redemptions reserve owned by some `account_id`.
	pub fn deposit_pending_redemption(account_id: &T::AccountId, amount: T::Balance) -> Deficit<T> {
		Deficit::from_pending_redemptions_reserve(account_id, amount)
	}
}

impl<T: Config> FundingInfo for Pallet<T> {
	type AccountId = T::AccountId;
	type Balance = T::Balance;

	fn total_balance_of(account_id: &Self::AccountId) -> Self::Balance {
		Self::total_balance_of(account_id)
	}

	fn total_onchain_funds() -> Self::Balance {
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
	) -> frame_support::dispatch::DispatchResult {
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

	fn burn_offchain(amount: Self::Balance) {
		let _remainder = Pallet::<T>::burn(amount).offset(Pallet::<T>::bridge_in(amount));
	}
}

impl<T: Config> AccountInfo for Pallet<T> {
	type AccountId = T::AccountId;
	type Amount = T::Amount;
	fn balance(account_id: &T::AccountId) -> T::Amount {
		Account::<T>::get(account_id).total()
	}

	fn bond(account_id: &T::AccountId) -> T::Amount {
		Account::<T>::get(account_id).bond()
	}

	fn liquid_funds(account_id: &T::AccountId) -> T::Amount {
		Account::<T>::get(account_id).liquid()
	}
}

impl<T: Config> cf_traits::Funding for Pallet<T> {
	type AccountId = T::AccountId;
	type Balance = T::Balance;
	type Handler = T::OnAccountFunded;

	fn credit_funds(account_id: &Self::AccountId, amount: Self::Balance) -> Self::Balance {
		let incoming = Self::bridge_in(amount);
		Self::settle(account_id, SignedImbalance::Positive(incoming));
		T::OnAccountFunded::on_account_funded(account_id, Self::balance(account_id));
		Self::total_balance_of(account_id)
	}

	fn try_initiate_redemption(
		account_id: &Self::AccountId,
		amount: Self::Balance,
	) -> Result<(), DispatchError> {
		ensure!(amount <= Self::liquid_funds(account_id), Error::<T>::InsufficientLiquidity);
		Self::settle(account_id, Self::deposit_pending_redemption(account_id, amount).into());
		T::OnAccountFunded::on_account_funded(account_id, Self::balance(account_id));

		Ok(())
	}

	fn finalize_redemption(account_id: &T::AccountId) -> Result<(), DispatchError> {
		// Get the total redemption amount.
		let imbalance: Surplus<T> = Self::try_withdraw_pending_redemption(account_id)?;
		let amount = imbalance.peek();
		let res = imbalance.offset(Self::bridge_out(amount));
		debug_assert!(
			res.try_none().is_ok(),
			"Bridge Out + Burned Fee should consume the entire Redemption amount."
		);
		Ok(())
	}

	fn revert_redemption(account_id: &Self::AccountId) -> Result<(), DispatchError> {
		// redemption reverts automatically when dropped
		let imbalance = Self::try_withdraw_pending_redemption(account_id)?;
		Self::settle(account_id, imbalance.into());
		T::OnAccountFunded::on_account_funded(account_id, Self::balance(account_id));
		Ok(())
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
		Pallet::<T>::deposit_event(Event::AccountReaped {
			who: account_id.clone(),
			dust_burned: dust,
		});
	}
}

pub struct FlipSlasher<T: Config>(PhantomData<T>);

impl<T: Config> FlipSlasher<T> {
	fn attempt_slash(
		account_id: &T::AccountId,
		account: FlipAccount<T::Balance>,
		slash_amount: T::Balance,
	) {
		if !slash_amount.is_zero() && account.can_be_slashed(slash_amount) {
			Pallet::<T>::settle(account_id, Pallet::<T>::burn(slash_amount).into());
			Pallet::<T>::deposit_event(Event::<T>::SlashingPerformed {
				who: account_id.clone(),
				amount: slash_amount,
			});
		}
	}
}

impl<T: Config> Slashing for FlipSlasher<T>
where
	BlockNumberFor<T>: Into<T::Balance>,
{
	type AccountId = T::AccountId;
	type BlockNumber = BlockNumberFor<T>;
	type Balance = T::Balance;

	fn slash(account_id: &Self::AccountId, blocks: Self::BlockNumber) {
		let account = Account::<T>::get(account_id);
		let slash_amount = Self::calculate_slash_amount(account_id, blocks);
		Self::attempt_slash(account_id, account, slash_amount);
	}

	fn slash_balance(account_id: &Self::AccountId, slash_amount: FlipBalance) {
		let account = Account::<T>::get(account_id);
		Self::attempt_slash(account_id, account, slash_amount.into());
	}

	fn calculate_slash_amount(
		account_id: &Self::AccountId,
		blocks: Self::BlockNumber,
	) -> Self::Balance {
		let account = Account::<T>::get(account_id);
		(SlashingRate::<T>::get() * account.bond / T::BlocksPerDay::get().into())
			.saturating_mul(blocks.into())
	}
}
