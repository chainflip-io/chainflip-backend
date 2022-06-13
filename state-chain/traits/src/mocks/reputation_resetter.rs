use sp_std::marker::PhantomData;

use crate::{Chainflip, ReputationResetter};

use super::{MockPallet, MockPalletStorage};

pub struct MockReputationResetter<T: Chainflip>(PhantomData<T>);

impl<T: Chainflip> MockPallet for MockReputationResetter<T> {
	const PREFIX: &'static [u8] = b"MockReputationResetter";
}

impl<T: Chainflip> MockReputationResetter<T> {
	pub fn set_reputation(validator: &T::ValidatorId, reputation: u64) {
		Self::put_storage(b"Reputation", validator, reputation);
	}

	pub fn get_reputation(validator: &T::ValidatorId) -> u64 {
		Self::get_storage(b"Reputation", validator).unwrap_or_default()
	}
}

impl<T: Chainflip> ReputationResetter for MockReputationResetter<T> {
	type ValidatorId = T::ValidatorId;

	fn reset_reputation(validator: &Self::ValidatorId) {
		Self::set_reputation(validator, 0)
	}
}
