use cf_traits::MissedAuthorshipSlots;
use codec::Decode;
use sp_consensus_aura::{Slot, AURA_ENGINE_ID};
use sp_runtime::DigestItem;

use crate::System;

frame_support::generate_storage_alias!(
	AuraSlotExtraction, LastSeenSlot => Value<Slot>
);

fn extract_slot_from_digest_item(item: &DigestItem) -> Option<Slot> {
	item.as_pre_runtime().and_then(|(id, mut data)| {
		if id == AURA_ENGINE_ID {
			Slot::decode(&mut data).ok()
		} else {
			None
		}
	})
}

pub struct MissedAuraSlots;

impl MissedAuthorshipSlots for MissedAuraSlots {
	fn missed_slots() -> sp_std::ops::Range<u64> {
		let authored = System::digest()
			.logs()
			.iter()
			.find_map(extract_slot_from_digest_item)
			.expect("Aura is not enabled;");

		let maybe_expected = LastSeenSlot::get().map(|last_seen| last_seen.saturating_add(1u64));
		LastSeenSlot::put(authored);
		if let Some(expected) = maybe_expected {
			(*expected)..(*authored)
		} else {
			log::info!("Not expecting any current slot.");
			Default::default()
		}
	}
}

#[cfg(test)]
mod test_missed_authorship_slots {
	use super::*;
	use codec::Encode;
	use frame_support::{
		construct_runtime, parameter_types,
		traits::{ConstU32, ConstU64, OnInitialize},
	};
	use sp_consensus_aura::ed25519::AuthorityId;
	use sp_runtime::{
		testing::{Header, UintAuthorityId},
		traits::IdentityLookup,
		BuildStorage, Digest,
	};

	type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
	type Block = frame_system::mocking::MockBlock<Test>;

	construct_runtime!(
		pub enum Test where
			Block = Block,
			NodeBlock = Block,
			UncheckedExtrinsic = UncheckedExtrinsic,
		{
			System: frame_system,
			Timestamp: pallet_timestamp,
			Aura: pallet_aura,
		}
	);

	parameter_types! {
		pub const BlockHashCount: u64 = 250;
	}
	impl frame_system::Config for Test {
		type BaseCallFilter = frame_support::traits::Everything;
		type BlockWeights = ();
		type BlockLength = ();
		type DbWeight = ();
		type Origin = Origin;
		type Index = u64;
		type BlockNumber = u64;
		type Call = Call;
		type Hash = sp_core::H256;
		type Hashing = ::sp_runtime::traits::BlakeTwo256;
		type AccountId = u64;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Header = Header;
		type Event = Event;
		type BlockHashCount = BlockHashCount;
		type Version = ();
		type PalletInfo = PalletInfo;
		type AccountData = ();
		type OnNewAccount = ();
		type OnKilledAccount = ();
		type SystemWeightInfo = ();
		type SS58Prefix = ();
		type OnSetCode = ();
		type MaxConsumers = frame_support::traits::ConstU32<5>;
	}

	const SLOT_DURATION: u64 = 6;

	impl pallet_timestamp::Config for Test {
		type Moment = u64;
		type OnTimestampSet = Aura;
		type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
		type WeightInfo = ();
	}

	impl pallet_aura::Config for Test {
		type AuthorityId = AuthorityId;
		type DisabledValidators = ();
		type MaxAuthorities = ConstU32<10>;
	}

	pub fn new_test_ext(authorities: Vec<u64>) -> frame_support::sp_io::TestExternalities {
		GenesisConfig {
			system: Default::default(),
			aura: AuraConfig {
				authorities: authorities
					.into_iter()
					.map(|a| UintAuthorityId(a).to_public_key())
					.collect(),
			},
		}
		.build_storage()
		.unwrap()
		.into()
	}

	#[test]
	fn test_slot_extraction() {
		let slot = Slot::from(42);
		assert_eq!(
			Some(slot),
			extract_slot_from_digest_item(&DigestItem::PreRuntime(
				AURA_ENGINE_ID,
				Encode::encode(&slot)
			))
		);
		assert_eq!(
			None,
			extract_slot_from_digest_item(&DigestItem::PreRuntime(*b"BORA", Encode::encode(&slot)))
		);
		assert_eq!(
			None,
			extract_slot_from_digest_item(&DigestItem::Other(b"SomethingElse".to_vec()))
		);
	}

	#[test]
	fn test_missed_slots() {
		// The genesis slot is some value greater than zero.
		const GENESIS_SLOT: u64 = 12345u64;

		fn simulate_block_authorship<F: Fn(Vec<u64>)>(block_number: u64, assertions: F) {
			// one slot per block, so slot == block_number
			let author_slot = Slot::from(GENESIS_SLOT + block_number);
			let pre_digest =
				Digest { logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, author_slot.encode())] };

			System::reset_events();
			System::initialize(&block_number, &System::parent_hash(), &pre_digest);
			System::on_initialize(block_number);
			Timestamp::on_initialize(block_number);
			assertions(<MissedAuraSlots as MissedAuthorshipSlots>::missed_slots().collect());
			Aura::on_initialize(block_number);
			Timestamp::set_timestamp(block_number);
		}

		new_test_ext(vec![0, 1, 2, 3, 4]).execute_with(|| {
			// No expected slot at genesis, so no missed slots.
			simulate_block_authorship(1, |missed_slots| {
				assert!(missed_slots.is_empty());
			});

			// Author block 3 - we missed slot 2.
			simulate_block_authorship(3, |missed_slots| {
				assert_eq!(missed_slots, [2].map(|x| GENESIS_SLOT + x));
			});
			assert_eq!(Aura::current_slot(), GENESIS_SLOT + 3);

			// Author for the next slot, assert we haven't missed a slot.
			simulate_block_authorship(4, |missed_slots| {
				assert!(missed_slots.is_empty());
			});
			assert_eq!(Aura::current_slot(), GENESIS_SLOT + 4);

			// Author for slot 7, assert we missed slots 5 and 6.
			simulate_block_authorship(7, |missed_slots| {
				assert_eq!(missed_slots, [5, 6].map(|x| GENESIS_SLOT + x));
			});
			assert_eq!(Aura::current_slot(), GENESIS_SLOT + 7);
		});
	}
}
