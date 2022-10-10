// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! The Polkadot runtime. This can be compiled with `#[no_std]`, ready for Wasm.

#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]

use runtime_common::{impl_runtime_weights, BlockHashCount, BlockLength};

use frame_support::dispatch::DispatchResult;
use frame_support::{
    construct_runtime, parameter_types,
    traits::{Contains, InstanceFilter},
    RuntimeDebug,
};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use polkadot_core_primitives::{AccountId, Balance, BlockNumber, Hash, Nonce, Signature};
use sp_runtime::traits::DispatchInfoOf;
use sp_runtime::traits::PostDispatchInfoOf;
use sp_runtime::transaction_validity::{
    TransactionValidity, TransactionValidityError, ValidTransaction,
};
use sp_runtime::{
    create_runtime_str, generic,
    traits::{AccountIdLookup, BlakeTwo256, SignedExtension, Verify},
    Perbill,
};
use sp_std::prelude::*;
#[cfg(any(feature = "std", test))]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

pub use frame_system::Call as SystemCall;
pub use pallet_balances::Call as BalancesCall;
#[cfg(feature = "std")]
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;

/// Constant values used within the runtime.
use polkadot_runtime_constants::currency::*;

// Weights used in the runtime.
mod weights;

//mod bag_thresholds;

impl_runtime_weights!(polkadot_runtime_constants);

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

// Polkadot version identifier;
/// Runtime version (Polkadot).
#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: create_runtime_str!("polkadot"),
    impl_name: create_runtime_str!("parity-polkadot"),
    authoring_version: 0,
    spec_version: 9290,
    impl_version: 0,
    apis: sp_version::create_apis_vec![[]],
    transaction_version: 14,
    state_version: 0,
};

/// Native version.
#[cfg(any(feature = "std", test))]
pub fn native_version() -> NativeVersion {
    NativeVersion {
        runtime_version: VERSION,
        can_author_with: Default::default(),
    }
}

pub struct BaseFilter;
impl Contains<Call> for BaseFilter {
    fn contains(call: &Call) -> bool {
        match call {
            // These modules are all allowed to be called by transactions:
            Call::System(_) | Call::Balances(_) | Call::Utility(_) | Call::Proxy(_) => true,
            // All pallets are allowed, but exhaustive match is defensive
            // in the case of adding new pallets.
        }
    }
}

parameter_types! {
    pub const Version: RuntimeVersion = VERSION;
    pub const SS58Prefix: u8 = 0;
}

impl frame_system::Config for Runtime {
    type BaseCallFilter = BaseFilter;
    type BlockWeights = BlockWeights;
    type BlockLength = BlockLength;
    type Origin = Origin;
    type Call = Call;
    type Index = Nonce;
    type BlockNumber = BlockNumber;
    type Hash = Hash;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = AccountIdLookup<AccountId, ()>;
    type Header = generic::Header<BlockNumber, BlakeTwo256>;
    type Event = Event;
    type BlockHashCount = BlockHashCount;
    type DbWeight = RocksDbWeight;
    type Version = Version;
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<Balance>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = weights::frame_system::WeightInfo<Runtime>;
    type SS58Prefix = SS58Prefix;
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

// type ScheduleOrigin = EitherOfDiverse<
//     EnsureRoot<AccountId>,
//     pallet_collective::EnsureProportionAtLeast<AccountId, CouncilCollective, 1, 2>,
// >;

// Used the compare the privilege of an origin inside the scheduler.
// pub struct OriginPrivilegeCmp;

// impl PrivilegeCmp<OriginCaller> for OriginPrivilegeCmp {
//     fn cmp_privilege(left: &OriginCaller, right: &OriginCaller) -> Option<Ordering> {
//         if left == right {
//             return Some(Ordering::Equal);
//         }

//         match (left, right) {
//             // Root is greater than anything.
//             (OriginCaller::system(frame_system::RawOrigin::Root), _) => Some(Ordering::Greater),
//             // Check which one has more yes votes.
//             (
//                 OriginCaller::Council(pallet_collective::RawOrigin::Members(l_yes_votes, l_count)),
//                 OriginCaller::Council(pallet_collective::RawOrigin::Members(r_yes_votes, r_count)),
//             ) => Some((l_yes_votes * r_count).cmp(&(r_yes_votes * l_count))),
//             // For every other origin we don't care, as they are not used for `ScheduleOrigin`.
//             _ => None,
//         }
//     }
// }

parameter_types! {
    pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
    pub const MaxLocks: u32 = 50;
    pub const MaxReserves: u32 = 50;
}

impl pallet_balances::Config for Runtime {
    type Balance = Balance;
    type DustRemoval = ();
    type Event = Event;
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type MaxLocks = MaxLocks;
    type MaxReserves = MaxReserves;
    type ReserveIdentifier = [u8; 8];
    type WeightInfo = weights::pallet_balances::WeightInfo<Runtime>;
}

parameter_types! {
    pub const TransactionByteFee: Balance = 10 * MILLICENTS;
    /// This value increases the priority of `Operational` transactions by adding
    /// a "virtual tip" that's equal to the `OperationalFeeMultiplier * final_fee`.
    pub const OperationalFeeMultiplier: u8 = 5;
}

// impl pallet_transaction_payment::Config for Runtime {
//     type Event = Event;
//     type OnChargeTransaction = CurrencyAdapter<Balances, DealWithFees<Runtime>>;
//     type OperationalFeeMultiplier = OperationalFeeMultiplier;
//     type WeightToFee = WeightToFee;
//     type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
//     type FeeMultiplierUpdate = SlowAdjustingFeeUpdate<Self>;
// }

parameter_types! {
    // signed config
    pub const SignedMaxSubmissions: u32 = 16;
    pub const SignedMaxRefunds: u32 = 16 / 4;
    // 40 DOTs fixed deposit..
    pub const SignedDepositBase: Balance = deposit(2, 0);
    // 0.01 DOT per KB of solution data.
    pub const SignedDepositByte: Balance = deposit(0, 10) / 1024;
    // Each good submission will get 1 DOT as reward
    pub SignedRewardBase: Balance = 1 * UNITS;
    pub BetterUnsignedThreshold: Perbill = Perbill::from_rational(5u32, 10_000);

    /// We take the top 22500 nominators as electing voters..
    pub const MaxElectingVoters: u32 = 22_500;
    /// ... and all of the validators as electable targets. Whilst this is the case, we cannot and
    /// shall not increase the size of the validator intentions.
    pub const MaxElectableTargets: u16 = u16::MAX;
}

/// Submits a transaction with the node's public and signature type. Adheres to the signed extension
/// format of the chain.
// impl<LocalCall> frame_system::offchain::CreateSignedTransaction<LocalCall> for Runtime
// where
//     Call: From<LocalCall>,
// {
//     fn create_transaction<C: frame_system::offchain::AppCrypto<Self::Public, Self::Signature>>(
//         call: Call,
//         public: <Signature as Verify>::Signer,
//         account: AccountId,
//         nonce: <Runtime as frame_system::Config>::Index,
//     ) -> Option<(Call, <UncheckedExtrinsic as ExtrinsicT>::SignaturePayload)> {
//         use sp_runtime::traits::StaticLookup;
//         // take the biggest period possible.
//         let period = BlockHashCount::get()
//             .checked_next_power_of_two()
//             .map(|c| c / 2)
//             .unwrap_or(2) as u64;

//         let current_block = System::block_number()
//             .saturated_into::<u64>()
//             // The `System::block_number` is initialized with `n+1`,
//             // so the actual block number is `n`.
//             .saturating_sub(1);
//         let tip = 0;
//         let extra: SignedExtra = (
//             frame_system::CheckNonZeroSender::<Runtime>::new(),
//             frame_system::CheckSpecVersion::<Runtime>::new(),
//             frame_system::CheckTxVersion::<Runtime>::new(),
//             frame_system::CheckGenesis::<Runtime>::new(),
//             frame_system::CheckMortality::<Runtime>::from(generic::Era::mortal(
//                 period,
//                 current_block,
//             )),
//             frame_system::CheckNonce::<Runtime>::from(nonce),
//             frame_system::CheckWeight::<Runtime>::new(),
//             pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(tip),
//             claims::PrevalidateAttests::<Runtime>::new(),
//         );
//         let raw_payload = SignedPayload::new(call, extra)
//             .map_err(|e| {
//                 log::warn!("Unable to create signed payload: {:?}", e);
//             })
//             .ok()?;
//         let signature = raw_payload.using_encoded(|payload| C::sign(payload, public))?;
//         let (call, extra, _) = raw_payload.deconstruct();
//         let address = <Runtime as frame_system::Config>::Lookup::unlookup(account);
//         Some((call, (address, signature, extra)))
//     }
// }

impl frame_system::offchain::SigningTypes for Runtime {
    type Public = <Signature as Verify>::Signer;
    type Signature = Signature;
}

parameter_types! {
    pub const ParathreadDeposit: Balance = 500 * DOLLARS;
    pub const MaxRetries: u32 = 3;
}

parameter_types! {
    pub Prefix: &'static [u8] = b"Pay DOTs to the Polkadot account:";
}

impl pallet_utility::Config for Runtime {
    type Event = Event;
    type Call = Call;
    type PalletsOrigin = OriginCaller;
    type WeightInfo = weights::pallet_utility::WeightInfo<Runtime>;
}

parameter_types! {
    // One storage item; key size 32, value size 8; .
    pub const ProxyDepositBase: Balance = deposit(1, 8);
    // Additional storage item size of 33 bytes.
    pub const ProxyDepositFactor: Balance = deposit(0, 33);
    pub const MaxProxies: u16 = 32;
    pub const AnnouncementDepositBase: Balance = deposit(1, 8);
    pub const AnnouncementDepositFactor: Balance = deposit(0, 66);
    pub const MaxPending: u16 = 32;
}

/// The type used to represent the kinds of proxying allowed.
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Encode,
    Decode,
    RuntimeDebug,
    MaxEncodedLen,
    scale_info::TypeInfo,
)]
pub enum ProxyType {
    Any = 0,
    NonTransfer = 1,
    Governance = 2,
    Staking = 3,
    // Skip 4 as it is now removed (was SudoBalances)
    IdentityJudgement = 5,
    CancelProxy = 6,
    Auction = 7,
}

#[cfg(test)]
mod proxy_type_tests {
    use super::*;

    #[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Encode, Decode, RuntimeDebug)]
    pub enum OldProxyType {
        Any,
        NonTransfer,
        Governance,
        Staking,
        SudoBalances,
        IdentityJudgement,
    }

    #[test]
    fn proxy_type_decodes_correctly() {
        for (i, j) in vec![
            (OldProxyType::Any, ProxyType::Any),
            (OldProxyType::NonTransfer, ProxyType::NonTransfer),
            (OldProxyType::Governance, ProxyType::Governance),
            (OldProxyType::Staking, ProxyType::Staking),
            (
                OldProxyType::IdentityJudgement,
                ProxyType::IdentityJudgement,
            ),
        ]
        .into_iter()
        {
            assert_eq!(i.encode(), j.encode());
        }
        assert!(ProxyType::decode(&mut &OldProxyType::SudoBalances.encode()[..]).is_err());
    }
}

impl Default for ProxyType {
    fn default() -> Self {
        Self::Any
    }
}
impl InstanceFilter<Call> for ProxyType {
    fn filter(&self, c: &Call) -> bool {
        match self {
            ProxyType::Any => true,
            ProxyType::NonTransfer => matches!(
                c,
                Call::System(..) |
				// Specifically omitting Vesting `vested_transfer`, and `force_vested_transfer`
				Call::Utility(..) |
				Call::Proxy(..)
            ),
            ProxyType::Governance => matches!(
                c,
                    | Call::Utility(..)
            ),
            ProxyType::Staking => {
                matches!(c, Call::Utility(..))
            }
            ProxyType::CancelProxy => {
                matches!(
                    c,
                    Call::Proxy(pallet_proxy::Call::reject_announcement { .. })
                )
            }
            _ => unreachable!(),
        }
    }
    fn is_superset(&self, o: &Self) -> bool {
        match (self, o) {
            (x, y) if x == y => true,
            (ProxyType::Any, _) => true,
            (_, ProxyType::Any) => false,
            (ProxyType::NonTransfer, _) => true,
            _ => false,
        }
    }
}

impl pallet_proxy::Config for Runtime {
    type Event = Event;
    type Call = Call;
    type Currency = Balances;
    type ProxyType = ProxyType;
    type ProxyDepositBase = ProxyDepositBase;
    type ProxyDepositFactor = ProxyDepositFactor;
    type MaxProxies = MaxProxies;
    type WeightInfo = weights::pallet_proxy::WeightInfo<Runtime>;
    type MaxPending = MaxPending;
    type CallHasher = BlakeTwo256;
    type AnnouncementDepositBase = AnnouncementDepositBase;
    type AnnouncementDepositFactor = AnnouncementDepositFactor;
}

construct_runtime! {
    pub enum Runtime where
        Block = Block,
        NodeBlock = polkadot_core_primitives::Block,
        UncheckedExtrinsic = UncheckedExtrinsic
    {
        // Basic stuff; balances is uncallable initially.
        System: frame_system::{Pallet, Call, Storage, Config, Event<T>} = 0,
        Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>} = 5,
        //TransactionPayment: pallet_transaction_payment::{Pallet, Storage, Event<T>} = 32,
        // Cunning utilities. Usable initially.
        Utility: pallet_utility::{Pallet, Call, Event} = 26,

        // Proxy module. Late addition.
        Proxy: pallet_proxy::{Pallet, Call, Storage, Event<T>} = 29,
    }
}

/// The address format for describing accounts.
pub type Address = sp_runtime::MultiAddress<AccountId, ()>;
/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
/// A Block signed with a Justification
pub type SignedBlock = generic::SignedBlock<Block>;
/// `BlockId` type as expected by this runtime.
pub type BlockId = generic::BlockId<Block>;
/// The `SignedExtension` to the basic transaction logic.
pub type SignedExtra = (
    frame_system::CheckNonZeroSender<Runtime>,
    frame_system::CheckSpecVersion<Runtime>,
    frame_system::CheckTxVersion<Runtime>,
    frame_system::CheckGenesis<Runtime>,
    frame_system::CheckMortality<Runtime>,
    frame_system::CheckNonce<Runtime>,
    frame_system::CheckWeight<Runtime>,
    DummyChargeTransactionPayment,
    DummyPrevalidateAttests,
);
/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic = generic::UncheckedExtrinsic<Address, Call, Signature, SignedExtra>;
/// The payload being signed in transactions.
pub type SignedPayload = generic::SignedPayload<Call, SignedExtra>;

#[derive(Encode, Decode, Debug, Clone, Eq, PartialEq, scale_info::TypeInfo)]
#[scale_info(skip_type_params(Runtime))]
pub struct DummyChargeTransactionPayment(#[codec(compact)] u128);

impl SignedExtension for DummyChargeTransactionPayment {
    const IDENTIFIER: &'static str = "DummyChargeTransactionPayment";
    type AccountId = AccountId;
    type Call = Call;
    type AdditionalSigned = ();
    type Pre = ();
    fn additional_signed(&self) -> sp_std::result::Result<(), TransactionValidityError> {
        Ok(())
    }
    fn validate(
        &self,
        _who: &Self::AccountId,
        _call: &Self::Call,
        _info: &DispatchInfoOf<Self::Call>,
        _len: usize,
    ) -> TransactionValidity {
        Ok(<ValidTransaction as Default>::default())
    }

    fn pre_dispatch(
        self,
        _who: &Self::AccountId,
        _call: &Self::Call,
        _info: &DispatchInfoOf<Self::Call>,
        _len: usize,
    ) -> Result<Self::Pre, TransactionValidityError> {
        Ok(())
    }

    fn post_dispatch(
        _maybe_pre: Option<Self::Pre>,
        _info: &DispatchInfoOf<Self::Call>,
        _post_info: &PostDispatchInfoOf<Self::Call>,
        _len: usize,
        _result: &DispatchResult,
    ) -> Result<(), TransactionValidityError> {
        Ok(())
    }
}

#[derive(Encode, Decode, Debug, Clone, Eq, PartialEq, scale_info::TypeInfo)]
#[scale_info(skip_type_params(Runtime))]
pub struct DummyPrevalidateAttests(());
impl SignedExtension for DummyPrevalidateAttests {
    const IDENTIFIER: &'static str = "Dummy";
    type AccountId = AccountId;
    type Call = Call;
    type AdditionalSigned = ();
    type Pre = ();
    fn additional_signed(&self) -> sp_std::result::Result<(), TransactionValidityError> {
        Ok(())
    }
    fn validate(
        &self,
        _who: &Self::AccountId,
        _call: &Self::Call,
        _info: &DispatchInfoOf<Self::Call>,
        _len: usize,
    ) -> TransactionValidity {
        Ok(<ValidTransaction as Default>::default())
    }

    fn pre_dispatch(
        self,
        _who: &Self::AccountId,
        _call: &Self::Call,
        _info: &DispatchInfoOf<Self::Call>,
        _len: usize,
    ) -> Result<Self::Pre, TransactionValidityError> {
        Ok(())
    }

    fn post_dispatch(
        _maybe_pre: Option<Self::Pre>,
        _info: &DispatchInfoOf<Self::Call>,
        _post_info: &PostDispatchInfoOf<Self::Call>,
        _len: usize,
        _result: &DispatchResult,
    ) -> Result<(), TransactionValidityError> {
        Ok(())
    }
}
