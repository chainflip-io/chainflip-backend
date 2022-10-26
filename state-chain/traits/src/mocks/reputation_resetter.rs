use core::marker::PhantomData;

use crate::{Chainflip, ReputationResetter};

pub struct MockReputationResetter<T: Chainflip>(PhantomData<T>);

impl<T: Chainflip> ReputationResetter for MockReputationResetter<T> {
	type ValidatorId = T::ValidatorId;

	fn reset_reputation(_validator: &Self::ValidatorId) {
		// do nothing
	}
}
