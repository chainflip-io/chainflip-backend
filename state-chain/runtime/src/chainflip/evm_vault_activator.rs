//! Key rotator to be used by the Validator pallet to control the rotation of multiple keys

use cf_chains::{evm::EvmCrypto, ChainCrypto};
use cf_traits::{AsyncResult, StartKeyActivationResult, VaultActivator};
use core::marker::PhantomData;

pub struct EvmVaultActivator<A, B> {
	_phantom: PhantomData<(A, B)>,
}

impl<A, B> VaultActivator<EvmCrypto> for EvmVaultActivator<A, B>
where
	A: VaultActivator<EvmCrypto>,
	B: VaultActivator<EvmCrypto, ValidatorId = A::ValidatorId>,
{
	type ValidatorId = A::ValidatorId;

	fn activate_key() {
		A::activate_key();
		B::activate_key();
	}

	/// Start all key rotations with the provided `candidates`.
	fn start_key_activation(
		new_key: <EvmCrypto as ChainCrypto>::AggKey,
		maybe_old_key: Option<<EvmCrypto as ChainCrypto>::AggKey>,
	) -> Vec<StartKeyActivationResult> {
		[
			A::start_key_activation(new_key, maybe_old_key),
			B::start_key_activation(new_key, maybe_old_key),
		]
		.concat()
	}

	fn status() -> AsyncResult<()> {
		let async_results = [A::status(), B::status()];

		// if any of the inner rotations are void, then the overall key rotation result is void.
		if async_results.iter().any(|item| matches!(item, AsyncResult::Void)) {
			return AsyncResult::Void
		}

		// We must wait until all of these are ready before we do any action
		if async_results.iter().all(|item| matches!(item, AsyncResult::Ready(..))) {
			AsyncResult::Ready(())
		} else {
			AsyncResult::Pending
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(outcome: AsyncResult<()>) {
		A::set_status(outcome);
		B::set_status(outcome);
	}
}
