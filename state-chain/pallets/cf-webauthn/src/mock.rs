use crate as pallet_cf_webauthn;
use frame_support::derive_impl;
use frame_system::EnsureRoot;
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		WebAuthn: pallet_cf_webauthn,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountId = AccountId32;
	type Lookup = IdentityLookup<AccountId32>;
}

impl pallet_cf_webauthn::Config for Test {
	type EnsureGovernance = EnsureRoot<AccountId32>;
	type WeightInfo = ();
}

pub const RP_ID: &[u8] = b"localhost";

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities = RuntimeGenesisConfig {
		system: Default::default(),
		web_authn: pallet_cf_webauthn::GenesisConfig {
			rp_id: RP_ID.to_vec().try_into().unwrap(),
			trust_anchors: Default::default(),
			_phantom: Default::default(),
		},
	}
	.build_storage()
	.unwrap()
	.into();
	// sr25519 host functions used by the baseline benchmark need a keystore.
	ext.register_extension(sp_keystore::KeystoreExt::new(
		sp_keystore::testing::MemoryKeystore::new(),
	));
	ext.execute_with(|| System::set_block_number(1));
	ext
}
