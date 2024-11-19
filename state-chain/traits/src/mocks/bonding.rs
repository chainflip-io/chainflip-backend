use crate::Bonding;
use frame_support::parameter_types;

use sp_std::collections::btree_map::BTreeMap;

pub type Amount = u128;
pub type ValidatorId = u64;

parameter_types! {
	pub storage AuthorityBonds: BTreeMap<ValidatorId, Amount> = Default::default();
}

pub struct MockBonder;

impl MockBonder {
	pub fn get_bond(account_id: &ValidatorId) -> Amount {
		AuthorityBonds::get().get(account_id).copied().unwrap_or(0)
	}
}

impl Bonding for MockBonder {
	type ValidatorId = ValidatorId;
	type Amount = Amount;

	fn update_bond(account_id: &Self::ValidatorId, bond: Self::Amount) {
		let mut authority_bonds = AuthorityBonds::get();
		authority_bonds.insert(*account_id, bond);
	}
}
