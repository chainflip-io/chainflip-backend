use cf_traits::MissedAuthorshipSlots;
use codec::Decode;
use sp_consensus_aura::{Slot as AuraSlot, AURA_ENGINE_ID};
use sp_runtime::DigestItem;
use sp_std::prelude::*;

use crate::{Aura, System};

struct AuraSlotExtraction;

impl AuraSlotExtraction {
	fn expected_slot() -> AuraSlot {
		Aura::current_slot() + 1
	}

	fn extract_slot_from_digest_item(item: &DigestItem) -> Option<AuraSlot> {
		item.as_pre_runtime().and_then(|(id, mut data)| {
			if id == AURA_ENGINE_ID {
				AuraSlot::decode(&mut data).ok()
			} else {
				None
			}
		})
	}

	fn current_slot_from_digests() -> Option<AuraSlot> {
		System::digest()
			.logs()
			.iter()
			.filter_map(Self::extract_slot_from_digest_item)
			.next()
	}
}

pub struct MissedAuraSlots;

impl MissedAuthorshipSlots for MissedAuraSlots {
	fn missed_slots() -> Vec<u64> {
		let expected = AuraSlotExtraction::expected_slot();
		if let Some(authored) = AuraSlotExtraction::current_slot_from_digests() {
			((*expected)..(*authored)).collect()
		} else {
			log::error!("No Aura authorship slot passed to runtime via digests!");
			vec![]
		}
	}
}

#[cfg(test)]
mod test_missed_authorship_slots {
	use super::*;
	use codec::Encode;
	use frame_support::{construct_runtime, parameter_types, traits::OnInitialize};
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
			System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
			Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent},
			Aura: pallet_aura::{Pallet, Config<T>},
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
	}

	parameter_types! {
		pub const MinimumPeriod: u64 = 1;
	}
	impl pallet_timestamp::Config for Test {
		type Moment = u64;
		type OnTimestampSet = Aura;
		type MinimumPeriod = MinimumPeriod;
		type WeightInfo = ();
	}

	impl pallet_aura::Config for Test {
		type AuthorityId = AuthorityId;
		type DisabledValidators = ();
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
		let slot = AuraSlot::from(42);
		assert_eq!(
			Some(slot),
			AuraSlotExtraction::extract_slot_from_digest_item(&DigestItem::PreRuntime(
				AURA_ENGINE_ID,
				Encode::encode(&slot)
			))
		);
		assert_eq!(
			None,
			AuraSlotExtraction::extract_slot_from_digest_item(&DigestItem::PreRuntime(
				*b"BORA",
				Encode::encode(&slot)
			))
		);
		assert_eq!(
			None,
			AuraSlotExtraction::extract_slot_from_digest_item(&DigestItem::Other(
				b"SomethingElse".to_vec()
			))
		);
	}

	fn simulate_block_authorship(block_number: u64, slot: u64) {
		let author_slot = AuraSlot::from(slot);
		let pre_digest =
			Digest { logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, author_slot.encode())] };

		System::reset_events();
		System::initialize(&block_number, &System::parent_hash(), &pre_digest);
	}

	#[test]
	fn test_missed_slots() {
		new_test_ext(vec![0, 1, 2, 3, 4]).execute_with(|| {
			let (block, slot) = (33u64, 3u64);
			simulate_block_authorship(block, slot);

			// Our author is authoring for slot 3. 0 has already been authored (genesis), so
			// 1 and 2 were skipped.
			assert_eq!(<MissedAuraSlots as MissedAuthorshipSlots>::missed_slots(), vec![1, 2]);

			// Aura updates after this - current slot should now be 3.
			<Aura as OnInitialize<u64>>::on_initialize(block);
			assert_eq!(Aura::current_slot(), slot);

			// Author for the next slot, assert we haven't missed a slot.
			let (block, slot) = (44u64, 4u64);
			simulate_block_authorship(block, slot);
			assert!(<MissedAuraSlots as MissedAuthorshipSlots>::missed_slots().is_empty());

			<Aura as OnInitialize<u64>>::on_initialize(block);
			assert_eq!(Aura::current_slot(), slot);

			// Author for slot 6, assert we missed slot 5.
			let (block, slot) = (66u64, 6u64);
			simulate_block_authorship(block, slot);
			assert_eq!(<MissedAuraSlots as MissedAuthorshipSlots>::missed_slots(), vec![5]);

			<Aura as OnInitialize<u64>>::on_initialize(block);
			assert_eq!(Aura::current_slot(), slot);
		})
	}
}
