use super::{MockPallet, MockPalletStorage};
use crate::{Bonding, Chainflip};
use codec::{Decode, Encode};
use core::marker::PhantomData;

pub struct MockBonder<Id, Amount>(PhantomData<(Id, Amount)>);
pub type MockBonderFor<T> =
	MockBonder<<T as frame_system::Config>::AccountId, <T as Chainflip>::Amount>;

impl<Id, Amount> MockPallet for MockBonder<Id, Amount> {
	const PREFIX: &'static [u8] = b"mocks//MockBonder";
}

const BOND: &[u8] = b"BOND";

impl<Id: Encode, Amount: Decode + Default> MockBonder<Id, Amount> {
	pub fn get_bond(account_id: &Id) -> Amount {
		Self::get_storage(BOND, account_id).unwrap_or_default()
	}
}

impl<Id: Encode, Amount: Encode> Bonding for MockBonder<Id, Amount> {
	type AccountId = Id;
	type Amount = Amount;

	fn update_bond(account_id: &Self::AccountId, bond: Amount) {
		Self::put_storage(BOND, account_id, bond);
	}
}
