use core::marker::PhantomData;

use crate::{Chainflip, ReputationResetter};

use super::{MockPallet, MockPalletStorage};

pub struct MockReputationResetter<T: Chainflip>(PhantomData<T>);

impl<T: Chainflip> MockPallet for MockReputationResetter<T> {
	const PREFIX: &'static [u8] = b"MockReputationResetter";
}

const REPUTATION: &[u8] = b"Reputation";

impl<T: Chainflip> MockReputationResetter<T> {
	pub fn reputation_was_reset() -> bool {
		Self::get_value(REPUTATION).unwrap_or_default()
	}
}

impl<T: Chainflip> ReputationResetter for MockReputationResetter<T> {
	type ValidatorId = T::ValidatorId;

	fn reset_reputation(_validator: &Self::ValidatorId) {
		Self::put_value(REPUTATION, true);
	}
}
