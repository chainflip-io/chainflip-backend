#![cfg(test)]

use crate::{self as pallet_cf_flip, BurnFlipAccount};
use cf_primitives::FlipBalance;
use cf_traits::{impl_mock_chainflip, impl_mock_waived_fees, Funding, WaivedFees};
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU128, ConstU8, HandleLifetime},
	weights::{ConstantMultiplier, IdentityFee},
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	Permill,
};

pub type AccountId = u64;

cf_traits::impl_mock_on_account_funded!(AccountId, u128);
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Flip: pallet_cf_flip,
		TransactionPayment: pallet_transaction_payment,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = BurnFlipAccount<Self>;
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<5>;
}

impl_mock_chainflip!(Test);

parameter_types! {
	pub const BlocksPerDay: u64 = 14400;
}

// Implement mock for WaivedFees
impl_mock_waived_fees!(AccountId, RuntimeCall);

impl pallet_cf_flip::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Balance = FlipBalance;
	type BlocksPerDay = BlocksPerDay;
	type OnAccountFunded = MockOnAccountFunded;
	type WeightInfo = ();
	type WaivedFees = WaivedFeesMock;
}

parameter_types! {
	pub const TransactionByteFee: FlipBalance = 1;
}

impl pallet_transaction_payment::Config for Test {
	type OnChargeTransaction = pallet_cf_flip::FlipTransactionPayment<Self>;
	type WeightToFee = IdentityFee<FlipBalance>;
	type FeeMultiplierUpdate = ();
	type OperationalFeeMultiplier = ConstU8<5>;
	type LengthToFee = ConstantMultiplier<u128, ConstU128<1_000_000>>;
	type RuntimeEvent = RuntimeEvent;
}

// Build genesis storage according to the mock runtime.
pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 456u64;
pub const CHARLIE: <Test as frame_system::Config>::AccountId = 789u64;

pub fn check_balance_integrity() -> bool {
	let accounts_total = pallet_cf_flip::Account::<Test>::iter_values()
		.map(|account| account.total())
		.sum::<FlipBalance>();
	let pending_redemptions_total =
		pallet_cf_flip::PendingRedemptionsReserve::<Test>::iter_values().sum::<FlipBalance>();
	let reserves_total = pallet_cf_flip::Reserve::<Test>::iter_values().sum::<FlipBalance>();

	(accounts_total + reserves_total + pending_redemptions_total) == Flip::onchain_funds()
}

pub type SlashingRateType = Permill;
pub type Bond = u128;
pub type Mint = u128;

#[derive(Clone, Debug)]
pub enum FlipOperation {
	MintExternal(FlipBalance, FlipBalance),
	BurnExternal(FlipBalance, FlipBalance),
	BurnReverts(FlipBalance),
	MintReverts(FlipBalance),
	CreditReverts(FlipBalance),
	DebitReverts(FlipBalance),
	BridgeInReverts(FlipBalance),
	BridgeOutReverts(FlipBalance),
	MintToReserve(FlipBalance),
	BurnFromReserve(FlipBalance),
	BurnFromAccount(AccountId, FlipBalance),
	MintToAccount(AccountId, FlipBalance),
	ExternalTransferOut(AccountId, FlipBalance),
	ExternalTransferIn(AccountId, FlipBalance),
	UpdateBalanceAndBond(AccountId, FlipBalance, FlipBalance),
	SlashAccount(AccountId, SlashingRateType, Bond, Mint, BlockNumberFor<Test>),
	AccountToAccount(AccountId, AccountId, FlipBalance, FlipBalance),
}

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system: Default::default(),
		flip: FlipConfig { total_issuance: 1_000, daily_slashing_rate: Permill::from_perthousand(1) },
		transaction_payment: Default::default(),
	},
	|| {
		// Seed with two funded accounts.
		frame_system::Provider::<Test>::created(&ALICE).unwrap();
		frame_system::Provider::<Test>::created(&BOB).unwrap();
		assert!(frame_system::Pallet::<Test>::account_exists(&ALICE));
		assert!(frame_system::Pallet::<Test>::account_exists(&BOB));
		assert!(!frame_system::Pallet::<Test>::account_exists(&CHARLIE));
		<Flip as Funding>::credit_funds(&ALICE, 100);
		<Flip as Funding>::credit_funds(&BOB, 50);

		assert_eq!(Flip::offchain_funds(), 850);
		check_balance_integrity();
	}
}
