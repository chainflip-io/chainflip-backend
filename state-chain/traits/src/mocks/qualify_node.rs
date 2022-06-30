use crate::QualifyNode;
use codec::{Decode, Encode};
use sp_std::marker::PhantomData;

use super::{MockPallet, MockPalletStorage};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct QualifyAll<Id>(PhantomData<Id>);

impl<Id> MockPallet for QualifyAll<Id> {
	const PREFIX: &'static [u8] = b"cf-mocks//QualifyAll";
}

impl<Id: Encode + Decode> QualifyAll<Id> {
	pub fn except(id: Id) {
		<Self as MockPalletStorage>::put_storage(b"EXCEPT", b"", id)
	}
}

impl<Id: Encode + Decode + Eq> QualifyNode for QualifyAll<Id> {
	type ValidatorId = Id;

	fn is_qualified(id: &Id) -> bool {
		Self::get_storage::<_, Id>(b"EXCEPT", b"")
			.map(|excluded| excluded != *id)
			.unwrap_or(true)
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_qualify_exclusion() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			assert!(QualifyAll::is_qualified(&1));
			QualifyAll::except(1);
			assert!(!QualifyAll::is_qualified(&1));
		});
	}
}
