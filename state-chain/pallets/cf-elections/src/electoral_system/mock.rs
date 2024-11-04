use frame_support::DefaultNoBound;

use super::*;

#[derive(DefaultNoBound)]
pub struct DummyAccess<T> {
	pub _phantom: core::marker::PhantomData<T>,
}

impl<T: ElectoralSystem> ElectionReadAccess for DummyAccess<T> {
	type ElectoralSystem = T;
	fn properties(
		&self,
	) -> Result<<Self::ElectoralSystem as ElectoralSystem>::ElectionProperties, CorruptStorageError>
	{
		Err(CorruptStorageError::new())
	}
	fn settings(
		&self,
	) -> Result<<Self::ElectoralSystem as ElectoralSystem>::ElectoralSettings, CorruptStorageError>
	{
		Err(CorruptStorageError::new())
	}
	fn state(
		&self,
	) -> Result<<Self::ElectoralSystem as ElectoralSystem>::ElectionState, CorruptStorageError> {
		Err(CorruptStorageError::new())
	}
	#[cfg(test)]
	fn election_identifier(
		&self,
	) -> Result<ElectionIdentifierOf<Self::ElectoralSystem>, CorruptStorageError> {
		Err(CorruptStorageError::new())
	}
}
