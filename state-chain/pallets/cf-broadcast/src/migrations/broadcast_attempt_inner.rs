use crate::*;
use sp_std::marker::PhantomData;

///
/// - Rename FailedBroadcastAttempt to BroadcastAttempt
/// - Migrates various broadcast structs to use BroadcastAttempt as a common inner between them
pub struct Migration<T: Config>(PhantomData<T>);

// Contain types of old version to decode into
mod v0 {

	pub type BroadcastAttemptId = u64;

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct TransmissionAttempt<T: Config<I>, I: 'static> {
		pub broadcast_id: BroadcastId,
		pub attempt_count: AttemptCount,
		pub unsigned_tx: UnsignedTransactionFor<T, I>,
		pub signer: T::ValidatorId,
		pub signed_tx: SignedTransactionFor<T, I>,
	}

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct TransactionSigningAttempt<T: Config<I>, I: 'static> {
		pub broadcast_id: BroadcastId,
		pub attempt_count: AttemptCount,
		pub unsigned_tx: UnsignedTransactionFor<T, I>,
		pub nominee: T::ValidatorId,
	}
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	// broadcast retry queue, only the inner name of the struct has changed, they decode to the same
	// thing, so no change required

	// all transaction signing attempts
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// Awaiting transaction signature collapsed
		AwaitingTransactionSignature::<T>::translate::<(
			v0::BroadcastAttemptId,
			v0::TransactionSigningAttempt,
		)>
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		Ok(())
	}
}
