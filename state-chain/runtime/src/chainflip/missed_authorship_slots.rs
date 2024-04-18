use cf_traits::MissedAuthorshipSlots;
use codec::Decode;
use frame_support::{sp_runtime::DigestItem, storage_alias};
use sp_consensus_aura::{Slot, AURA_ENGINE_ID};

use crate::System;

#[storage_alias]
type LastSeenSlot = StorageValue<AuraSlotExtraction, Slot>;

// https://github.com/chainflip-io/substrate/blob/c172d0f683fab3792b90d876fd6ca27056af9fe9/frame/aura/src/lib.rs#L179
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
		match (
			System::digest().logs().iter().find_map(extract_slot_from_digest_item),
			LastSeenSlot::get().map(|last_seen| last_seen.saturating_add(1u64)),
		) {
			(None, _) => {
				log::warn!("No Aura block author slot can be determined");
				Default::default()
			},
			(Some(authored), maybe_expected) => {
				LastSeenSlot::put(authored);
				if let Some(expected) = maybe_expected {
					log::debug!("Expected Aura slot {:?}, got {:?}.", expected, authored);
					(*expected)..(*authored)
				} else {
					log::info!("Not expecting any current Aura author slot, got {:?}.", authored);
					Default::default()
				}
			},
		}
	}
}

#[cfg(test)]
mod test_missed_authorship_slots {
	use super::*;
	use codec::Encode;
	use frame_support::{
		construct_runtime, derive_impl, parameter_types,
		sp_runtime::{testing::UintAuthorityId, traits::IdentityLookup, BuildStorage, Digest},
		traits::{ConstU32, ConstU64, OnInitialize},
	};
	use sp_consensus_aura::ed25519::AuthorityId;
	use sp_core::ConstBool;

	type Block = frame_system::mocking::MockBlock<Test>;

	construct_runtime!(
		pub enum Test
		{
			System: frame_system,
			Timestamp: pallet_timestamp,
			Aura: pallet_aura,
		}
	);

	parameter_types! {
		pub const BlockHashCount: u64 = 250;
	}

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
	impl frame_system::Config for Test {
		type BaseCallFilter = frame_support::traits::Everything;
		type BlockWeights = ();
		type BlockLength = ();
		type DbWeight = ();
		type RuntimeOrigin = RuntimeOrigin;
		type Nonce = u64;
		type RuntimeCall = RuntimeCall;
		type Hash = sp_core::H256;
		type Hashing = ::sp_runtime::traits::BlakeTwo256;
		type AccountId = u64;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Block = Block;
		type RuntimeEvent = RuntimeEvent;
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
		type AllowMultipleBlocksPerSlot = ConstBool<false>;
	}

	pub fn new_test_ext(authorities: Vec<u64>) -> sp_io::TestExternalities {
		RuntimeGenesisConfig {
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
			assertions(<MissedAuraSlots as MissedAuthorshipSlots>::missed_slots().collect());
			Aura::on_initialize(block_number);
		}

		new_test_ext(vec![0, 1, 2, 3, 4]).execute_with(|| {
			// No expected slot at genesis, so no missed slots.
			simulate_block_authorship(1, |missed_slots| {
				assert!(missed_slots.is_empty());
			});

			let to_slot = |x| GENESIS_SLOT + x;

			// Author block 3 - we missed slot 2.
			simulate_block_authorship(3, |missed_slots| {
				assert_eq!(missed_slots, [2].map(to_slot));
			});
			assert_eq!(Aura::current_slot(), to_slot(3));

			// Author for the next slot, assert we haven't missed a slot.
			simulate_block_authorship(4, |missed_slots| {
				assert!(missed_slots.is_empty());
			});
			assert_eq!(Aura::current_slot(), to_slot(4));

			// Author for slot 7, assert we missed slots 5 and 6.
			simulate_block_authorship(7, |missed_slots| {
				assert_eq!(missed_slots, [5, 6].map(to_slot));
			});
			assert_eq!(Aura::current_slot(), to_slot(7));
		});
	}
}
