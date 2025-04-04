// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use super::*;
use cf_runtime_utilities::StorageDecodeVariant;
use cf_traits::{GetBlockHeight, StartKeyActivationResult, VaultActivator};

impl<T: Config<I>, I: 'static> VaultActivator<<T::Chain as Chain>::ChainCrypto> for Pallet<T, I> {
	type ValidatorId = T::ValidatorId;

	/// Get the status of the current key generation
	fn status() -> AsyncResult<()> {
		if let Some(status_variant) = PendingVaultActivation::<T, I>::decode_variant() {
			match status_variant {
				VaultActivationStatusVariant::AwaitingActivation => AsyncResult::Pending,
				VaultActivationStatusVariant::ActivationFailedAwaitingGovernance =>
					AsyncResult::Pending,
				VaultActivationStatusVariant::Complete => AsyncResult::Ready(()),
			}
		} else {
			AsyncResult::Void
		}
	}

	fn activate_key() {
		if VaultStartBlockNumbers::<T, I>::iter_keys().next().is_some() &&
			!matches!(
				PendingVaultActivation::<T, I>::decode_variant(),
				Some(VaultActivationStatusVariant::ActivationFailedAwaitingGovernance)
			) {
			Self::activate_new_key_for_chain(T::ChainTracking::get_block_height());
		}
	}

	fn start_key_activation(
		new_public_key: AggKeyFor<T, I>,
		maybe_old_public_key: Option<AggKeyFor<T, I>>,
	) -> Vec<StartKeyActivationResult> {
		// If this storage item exists, it means this chain is already active we will rotate
		// normally
		if VaultStartBlockNumbers::<T, I>::iter_keys().next().is_some() {
			match <T::SetAggKeyWithAggKey as SetAggKeyWithAggKey<_>>::new_unsigned(
				maybe_old_public_key,
				new_public_key,
			) {
				Ok(Some(activation_call)) => {
					// we need to sign and submit the rotation call
					// reporting back the request_id of the tss such that we can complete the
					// rotation when that request is completed
					let (_, tss_request_id) =
						T::Broadcaster::threshold_sign_and_broadcast_rotation_tx(
							activation_call,
							new_public_key,
						);
					// since vaults are activated only when the tss completes we need to initiate
					// the activation
					PendingVaultActivation::<T, I>::put(
						VaultActivationStatus::<T, I>::AwaitingActivation { new_public_key },
					);
					vec![StartKeyActivationResult::Normal(tss_request_id)]
				},
				Ok(None) => {
					// This can happen if, for example, on a utxo chain there are no funds that
					// need to be swept.
					Self::activate_key();
					vec![StartKeyActivationResult::ActivationTxNotRequired]
				},
				Err(err) => {
					log::error!(
						"Unexpected failure during {} vault activation. Error: {:?}",
						<T::Chain as cf_chains::Chain>::NAME,
						err,
					);
					PendingVaultActivation::<T, I>::put(
						VaultActivationStatus::<T, I>::ActivationFailedAwaitingGovernance {
							new_public_key,
						},
					);
					Self::deposit_event(Event::<T, I>::ActivationTxFailedAwaitingGovernance {
						new_public_key,
					});
					vec![StartKeyActivationResult::ActivationTxFailed]
				},
			}
		}
		// If the chain is not active yet, we check this flag to decide whether we want to activate
		// this chain during this epoch rotation
		else if ChainInitialized::<T, I>::get() {
			// VaultStartBlockNumbers being empty means we are bootstrapping the vault.
			PendingVaultActivation::<T, I>::put(
				VaultActivationStatus::<T, I>::AwaitingActivation { new_public_key },
			);
			Self::deposit_event(Event::<T, I>::AwaitingGovernanceActivation { new_public_key });
			vec![StartKeyActivationResult::FirstVault]
		}
		// The case where the ChainInitialized flag is not set, we skip activation for this chain
		// and complete rotation since this chain is still not ready to be activated yet.
		else {
			PendingVaultActivation::<T, I>::put(VaultActivationStatus::<T, I>::Complete);
			vec![StartKeyActivationResult::ChainNotInitialized]
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(outcome: AsyncResult<()>) {
		match outcome {
			AsyncResult::Pending => {
				PendingVaultActivation::<T, I>::put(
					VaultActivationStatus::<T, I>::AwaitingActivation {
						new_public_key: Default::default(),
					},
				);
			},
			AsyncResult::Ready(_) => {
				PendingVaultActivation::<T, I>::put(VaultActivationStatus::<T, I>::Complete);
			},
			AsyncResult::Void => {
				PendingVaultActivation::<T, I>::kill();
			},
		}
	}
}
