use frame_system::{ensure_signed, pallet_prelude::OriginFor, RawOrigin};
use sp_runtime::DispatchError;
use sp_std::marker::PhantomData;

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	error::BadOrigin,
	traits::{EnsureOrigin, StoredMap},
	RuntimeDebug,
};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

/// Account types of the Chainflip network.
///
/// Chainflip's network is permissioned and only accessible to owners of accounts with staked Flip.
/// In addition to staking, the account owner is required to register their account as one fo the
/// account types, to indicate the role they intend to play in the network.
#[derive(PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebug, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum AccountType {
	/// The default account type - indicates a bare account with no special role or permissions.
	Undefined,
	/// Validators are responsible for the maintainance and operation of the Chainflip network. See
	/// [ValidatorAccountState] for a further breakdown of this role.
	Validator(ValidatorAccountData),
	/// Liquidity providers can deposit assets and deploy them in trading pools.
	LiquidityProvider,
	/// Relayers submit swap intents on behalf of users.
	Relayer,
}

impl Default for AccountType {
	fn default() -> Self {
		AccountType::Undefined
	}
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebug, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct ValidatorAccountData {
	pub state: ValidatorAccountState,
	pub is_active_bidder: bool,
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebug, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum ValidatorAccountState {
	/// Current authorities are those validator nodes whose stake is (partially) bonded and whose
	/// responsibilities include participating in block authorship, witnessing, and threshold
	/// signature ceremonies.
	CurrentAuthority,
	/// Historical authority status implies Backup. It also implies that some bond is still being
	/// held and that the validator may be required to participate in ceremonies using the keys
	/// from an unexpired epoch.
	HistoricalAuthority,
	/// Backup state implies that the node is staked and may bid for an auction slot and compete
	/// for backup rewards.
	Backup,
}

impl Default for ValidatorAccountState {
	fn default() -> Self {
		ValidatorAccountState::Backup
	}
}

impl ValidatorAccountState {
	pub fn is_authority(&self) -> bool {
		matches!(self, Self::CurrentAuthority)
	}

	pub fn is_backup(&self) -> bool {
		matches!(self, Self::HistoricalAuthority | Self::Backup)
	}
}

#[derive(Default, PartialEq, Eq, Clone, Copy, Encode, Decode, TypeInfo, RuntimeDebug)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct ChainflipAccountData {
	pub account_type: AccountType,
}

/// Apis specific to accounts of type [AccountType::Validator].
pub trait ValidatorAccount {
	type AccountId;

	/// Tries to get the validator's account state. Can fail if the account is not a validator
	/// account.
	fn try_get_validator_state(
		account_id: &Self::AccountId,
	) -> Result<ValidatorAccountState, AccountError>;
	/// Tries to mutate the validator's account state. Can fail if the account is not a validator
	/// account.
	fn try_mutate_validator_state<R>(
		account_id: &Self::AccountId,
		f: impl FnOnce(&mut ValidatorAccountState) -> R,
	) -> Result<R, AccountError>;
	/// Set the node to be a current authority
	fn set_current_authority(account_id: &Self::AccountId);
	/// Sets the authority state to historical
	fn set_historical_authority(account_id: &Self::AccountId);
	/// Sets the current authority to the historical authority, should be called
	/// once the authority has no more active epochs
	fn from_historical_to_backup(account_id: &Self::AccountId);
}

#[derive(Debug)]
pub enum AccountError {
	InvalidAccountType,
	AccountNotInitialised,
	/// Accounts can only be upgraded from the initial [AccountType::Undefined] state.
	InvalidAccountTypeUpgrade,
	AccountDataMutationFailed(DispatchError),
	Other(DispatchError),
}

impl From<DispatchError> for AccountError {
	fn from(e: DispatchError) -> Self {
		AccountError::Other(e)
	}
}

impl From<AccountError> for DispatchError {
	fn from(e: AccountError) -> Self {
		match e {
			AccountError::InvalidAccountType => DispatchError::Other("InvalidAccountType"),
			AccountError::AccountNotInitialised => DispatchError::Other("UnitialisedAccount"),
			AccountError::InvalidAccountTypeUpgrade =>
				DispatchError::Other("InvalidAccountTypeUpgrade"),
			AccountError::AccountDataMutationFailed(e) => e,
			AccountError::Other(e) => e,
		}
	}
}

/// Chainflip-specific wrapper around [frame_system]'s account data accessors.
pub struct ChainflipAccountStore<T>(PhantomData<T>);

impl<T: frame_system::Config<AccountData = ChainflipAccountData>> ChainflipAccountStore<T> {
	/// Get the account data for the given account id.
	///
	/// Note: if the account does not exist, returns the [Default].
	pub fn get(account_id: &T::AccountId) -> ChainflipAccountData {
		frame_system::Pallet::<T>::get(account_id)
	}

	/// Upgrade an account from its initial [AccountType::Undefined] state.
	///
	/// Fails if the account has already been upgraded.
	pub fn upgrade_account_type(
		account_id: &<T as frame_system::Config>::AccountId,
		account_type: AccountType,
	) -> Result<(), AccountError> {
		frame_system::Pallet::<T>::try_mutate_exists(account_id, |maybe_account_data| {
			// The system pallet treats all accounts as non-existent if their AccountData is
			// Default. So instead of just checking for None, we also need to check for
			// Some(Default::default()).
			if maybe_account_data
				.replace(ChainflipAccountData { account_type })
				.unwrap_or_default() !=
				Default::default()
			{
				Err(AccountError::InvalidAccountTypeUpgrade)
			} else {
				Ok(())
			}
		})
	}
	/// Try to apply a mutation to the account data.
	///
	/// Fails if the account has not been initialised. If the provided closure returns an `Err`,
	/// does not mutate.
	pub fn try_mutate_account_data<
		R,
		E: Into<DispatchError>,
		F: FnOnce(&mut ChainflipAccountData) -> Result<R, E>,
	>(
		account_id: &T::AccountId,
		f: F,
	) -> Result<R, AccountError> {
		// Note this `try_mutate_exists` is *not* analogous to the storage method with the same
		// name. Notably, the `Account` storage in `frame_system` is *Value* storage, so if the
		// returned value is equal to the default value, it will be coerced to `None` before being
		// passed into the closure!
		frame_system::Pallet::<T>::try_mutate_exists(account_id, |maybe_account_data| {
			maybe_account_data.as_mut().map_or(
				Err(AccountError::AccountNotInitialised),
				|account_data| {
					f(account_data).map_err(|e| AccountError::AccountDataMutationFailed(e.into()))
				},
			)
		})
	}
}

impl<T: frame_system::Config<AccountData = ChainflipAccountData>> ValidatorAccount
	for ChainflipAccountStore<T>
{
	type AccountId = T::AccountId;

	fn try_mutate_validator_state<R>(
		account_id: &Self::AccountId,
		f: impl FnOnce(&mut ValidatorAccountState) -> R,
	) -> Result<R, AccountError> {
		Self::try_mutate_account_data(account_id, |account_data| match account_data.account_type {
			AccountType::Validator(ValidatorAccountData { ref mut state, .. }) => Ok(f(state)),
			_ => Err(AccountError::InvalidAccountType),
		})
	}

	fn try_get_validator_state(
		account_id: &Self::AccountId,
	) -> Result<ValidatorAccountState, AccountError> {
		match ChainflipAccountStore::<T>::get(account_id).account_type {
			AccountType::Validator(ValidatorAccountData { state, .. }) => Ok(state),
			_ => Err(AccountError::InvalidAccountType),
		}
	}

	/// Set the account state to Validator.
	///
	/// **Only call this on Validator accounts.**
	fn set_current_authority(account_id: &Self::AccountId) {
		log::debug!("Setting current authority {:?}", account_id);
		Self::try_mutate_validator_state(account_id, |state| {
			*state = ValidatorAccountState::CurrentAuthority;
		})
		.unwrap_or_else(|e| {
			log::error!("Failed to set current authority {:?}: {:?}", account_id, e);
		});
	}

	/// Set the account state to HistoricalAuthority.
	///
	/// **Only call this on Validator accounts.**
	fn set_historical_authority(account_id: &Self::AccountId) {
		Self::try_mutate_validator_state(account_id, |state| {
			*state = ValidatorAccountState::HistoricalAuthority;
		})
		.unwrap_or_else(|e| {
			log::error!("Failed to set historical authority {:?}: {:?}", account_id, e);
		});
	}

	/// Set the account state to Backup.
	///
	/// **Only call this on Validator accounts.**
	fn from_historical_to_backup(account_id: &Self::AccountId) {
		Self::try_mutate_validator_state(account_id, |state| match state {
			ValidatorAccountState::HistoricalAuthority => {
				*state = ValidatorAccountState::Backup;
			},
			_ => {
				const ERROR_MESSAGE: &str = "Attempted to transition to backup from historical, on a non-historical authority";
				log::error!("{}", ERROR_MESSAGE);
				#[cfg(test)]
				panic!("{}", ERROR_MESSAGE);
			},
		})
		.unwrap_or_else(|e| {
			log::error!(
				"Failed to convert account from historical to backup {:?}: {:?}",
				account_id,
				e
			);
		});
	}
}

macro_rules! define_ensure_origin {
	( $fn_name:ident, $struct_name:ident, $account_variant:pat ) => {
		/// Implements EnsureOrigin, enforcing the correct [AccountType].
		pub struct $struct_name<T>(PhantomData<T>);

		impl<T> EnsureOrigin<OriginFor<T>> for $struct_name<T>
		where
			T: frame_system::Config<AccountData = ChainflipAccountData>,
		{
			type Success = ();

			fn try_origin(o: OriginFor<T>) -> Result<Self::Success, OriginFor<T>> {
				match o.clone().into() {
					Ok(RawOrigin::Signed(ref account_id)) =>
						match ChainflipAccountStore::<T>::get(account_id).account_type {
							$account_variant => Ok(()),
							_ => Err(o),
						},
					Ok(o) => Err(o.into()),
					Err(o) => Err(o),
				}
			}
		}

		/// Ensure that the origin is signed and that the signer operates the correct [AccountType].
		pub fn $fn_name<T>(o: OriginFor<T>) -> Result<T::AccountId, BadOrigin>
		where
			T: frame_system::Config<AccountData = ChainflipAccountData>,
		{
			ensure_signed(o).and_then(|account_id| {
				match ChainflipAccountStore::<T>::get(&account_id).account_type {
					$account_variant => Ok(account_id),
					_ => Err(BadOrigin),
				}
			})
		}
	};
}

define_ensure_origin!(ensure_relayer, EnsureRelayer, AccountType::Relayer);
define_ensure_origin!(ensure_validator, EnsureValidator, AccountType::Validator { .. });
define_ensure_origin!(
	ensure_liquidity_provider,
	EnsureLiquidityProvider,
	AccountType::LiquidityProvider
);

#[cfg(test)]
mod test {
	use frame_support::traits::{ConstU16, ConstU64, HandleLifetime};
	use frame_system::Provider;
	use sp_core::H256;
	use sp_runtime::{
		testing::Header,
		traits::{BlakeTwo256, IdentityLookup},
	};

	use super::*;

	type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
	type Block = frame_system::mocking::MockBlock<Test>;
	type AccountId = u64;

	frame_support::construct_runtime!(
		pub enum Test where
			Block = Block,
			NodeBlock = Block,
			UncheckedExtrinsic = UncheckedExtrinsic,
		{
			System: frame_system,
		}
	);

	impl frame_system::Config for Test {
		type BaseCallFilter = frame_support::traits::Everything;
		type BlockWeights = ();
		type BlockLength = ();
		type DbWeight = ();
		type Origin = Origin;
		type Call = Call;
		type Index = u64;
		type BlockNumber = u64;
		type Hash = H256;
		type Hashing = BlakeTwo256;
		type AccountId = AccountId;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Header = Header;
		type Event = Event;
		type BlockHashCount = ConstU64<250>;
		type Version = ();
		type PalletInfo = PalletInfo;
		type AccountData = ChainflipAccountData;
		type OnNewAccount = ();
		type OnKilledAccount = ();
		type SystemWeightInfo = ();
		type SS58Prefix = ConstU16<42>;
		type OnSetCode = ();
		type MaxConsumers = frame_support::traits::ConstU32<5>;
	}

	pub fn new_test_ext() -> sp_io::TestExternalities {
		frame_system::GenesisConfig::default().build_storage::<Test>().unwrap().into()
	}

	const ALICE: u64 = 1;
	const BOB: u64 = 2;
	const CHARLIE: u64 = 3;

	#[test]
	fn test_ensure_origin_struct() {
		new_test_ext().execute_with(|| {
			// Root and none should be invalid.
			EnsureRelayer::<Test>::ensure_origin(OriginFor::<Test>::root()).unwrap_err();
			EnsureRelayer::<Test>::ensure_origin(OriginFor::<Test>::none()).unwrap_err();
			EnsureValidator::<Test>::ensure_origin(OriginFor::<Test>::root()).unwrap_err();
			EnsureValidator::<Test>::ensure_origin(OriginFor::<Test>::none()).unwrap_err();
			EnsureLiquidityProvider::<Test>::ensure_origin(OriginFor::<Test>::root()).unwrap_err();
			EnsureLiquidityProvider::<Test>::ensure_origin(OriginFor::<Test>::none()).unwrap_err();

			// Validation should fail for non-existent accounts.
			EnsureRelayer::<Test>::ensure_origin(OriginFor::<Test>::signed(ALICE)).unwrap_err();
			EnsureRelayer::<Test>::ensure_origin(OriginFor::<Test>::signed(BOB)).unwrap_err();
			EnsureRelayer::<Test>::ensure_origin(OriginFor::<Test>::signed(CHARLIE)).unwrap_err();

			// Create the accounts.
			Provider::<Test>::created(&ALICE).unwrap();
			Provider::<Test>::created(&BOB).unwrap();
			Provider::<Test>::created(&CHARLIE).unwrap();

			// Validation should fail for uninitalised accounts.
			EnsureRelayer::<Test>::ensure_origin(OriginFor::<Test>::signed(ALICE)).unwrap_err();
			EnsureRelayer::<Test>::ensure_origin(OriginFor::<Test>::signed(BOB)).unwrap_err();
			EnsureRelayer::<Test>::ensure_origin(OriginFor::<Test>::signed(CHARLIE)).unwrap_err();

			// Upgrade the accounts.
			ChainflipAccountStore::<Test>::upgrade_account_type(&ALICE, AccountType::Relayer)
				.unwrap();
			ChainflipAccountStore::<Test>::upgrade_account_type(
				&BOB,
				AccountType::Validator {
					state: ValidatorAccountState::Backup,
					is_active_bidder: false,
				},
			)
			.unwrap();
			ChainflipAccountStore::<Test>::upgrade_account_type(
				&CHARLIE,
				AccountType::LiquidityProvider,
			)
			.unwrap();

			// Each account should validate as the correct account type and fail otherwise.
			EnsureRelayer::<Test>::ensure_origin(OriginFor::<Test>::signed(ALICE)).unwrap();
			EnsureValidator::<Test>::ensure_origin(OriginFor::<Test>::signed(ALICE)).unwrap_err();
			EnsureLiquidityProvider::<Test>::ensure_origin(OriginFor::<Test>::signed(ALICE))
				.unwrap_err();
			EnsureRelayer::<Test>::ensure_origin(OriginFor::<Test>::signed(BOB)).unwrap_err();
			EnsureValidator::<Test>::ensure_origin(OriginFor::<Test>::signed(BOB)).unwrap();
			EnsureLiquidityProvider::<Test>::ensure_origin(OriginFor::<Test>::signed(BOB))
				.unwrap_err();
			EnsureRelayer::<Test>::ensure_origin(OriginFor::<Test>::signed(CHARLIE)).unwrap_err();
			EnsureValidator::<Test>::ensure_origin(OriginFor::<Test>::signed(CHARLIE)).unwrap_err();
			EnsureLiquidityProvider::<Test>::ensure_origin(OriginFor::<Test>::signed(CHARLIE))
				.unwrap();
		});
	}

	#[test]
	fn test_ensure_origin_fn() {
		new_test_ext().execute_with(|| {
			// Root and none should be invalid.
			ensure_relayer::<Test>(OriginFor::<Test>::root()).unwrap_err();
			ensure_relayer::<Test>(OriginFor::<Test>::none()).unwrap_err();
			ensure_validator::<Test>(OriginFor::<Test>::root()).unwrap_err();
			ensure_validator::<Test>(OriginFor::<Test>::none()).unwrap_err();
			ensure_liquidity_provider::<Test>(OriginFor::<Test>::root()).unwrap_err();
			ensure_liquidity_provider::<Test>(OriginFor::<Test>::none()).unwrap_err();

			// Validation should fail for non-existent accounts.
			ensure_relayer::<Test>(OriginFor::<Test>::signed(ALICE)).unwrap_err();
			ensure_relayer::<Test>(OriginFor::<Test>::signed(BOB)).unwrap_err();
			ensure_relayer::<Test>(OriginFor::<Test>::signed(CHARLIE)).unwrap_err();

			// Create the accounts.
			Provider::<Test>::created(&ALICE).unwrap();
			Provider::<Test>::created(&BOB).unwrap();
			Provider::<Test>::created(&CHARLIE).unwrap();

			// Validation should fail for uninitalised accounts.
			ensure_relayer::<Test>(OriginFor::<Test>::signed(ALICE)).unwrap_err();
			ensure_relayer::<Test>(OriginFor::<Test>::signed(BOB)).unwrap_err();
			ensure_relayer::<Test>(OriginFor::<Test>::signed(CHARLIE)).unwrap_err();

			// Upgrade the accounts.
			ChainflipAccountStore::<Test>::upgrade_account_type(&ALICE, AccountType::Relayer)
				.unwrap();
			ChainflipAccountStore::<Test>::upgrade_account_type(
				&BOB,
				AccountType::Validator {
					state: ValidatorAccountState::Backup,
					is_active_bidder: false,
				},
			)
			.unwrap();
			ChainflipAccountStore::<Test>::upgrade_account_type(
				&CHARLIE,
				AccountType::LiquidityProvider,
			)
			.unwrap();

			// Each account should validate as the correct account type and fail otherwise.
			ensure_relayer::<Test>(OriginFor::<Test>::signed(ALICE)).unwrap();
			ensure_validator::<Test>(OriginFor::<Test>::signed(ALICE)).unwrap_err();
			ensure_liquidity_provider::<Test>(OriginFor::<Test>::signed(ALICE)).unwrap_err();
			ensure_relayer::<Test>(OriginFor::<Test>::signed(BOB)).unwrap_err();
			ensure_validator::<Test>(OriginFor::<Test>::signed(BOB)).unwrap();
			ensure_liquidity_provider::<Test>(OriginFor::<Test>::signed(BOB)).unwrap_err();
			ensure_relayer::<Test>(OriginFor::<Test>::signed(CHARLIE)).unwrap_err();
			ensure_validator::<Test>(OriginFor::<Test>::signed(CHARLIE)).unwrap_err();
			ensure_liquidity_provider::<Test>(OriginFor::<Test>::signed(CHARLIE)).unwrap();
		});
	}
}
