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

//! Enables the network lockdown and removes inactive independent validators, so that the only
//! nodes left on the engines' p2p network are operator-managed validators and the independent
//! validators that are still running.

use crate::{AccountId, BlockNumber, Runtime};
use cf_primitives::{AccountRole, SECONDS_PER_BLOCK};
use cf_traits::HistoricalEpoch;
use frame_support::{
	migrations::VersionedMigration, traits::UncheckedOnRuntimeUpgrade, weights::Weight,
};
use sp_std::vec::Vec;

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

pub type Migration = VersionedMigration<
	10,
	11,
	NetworkLockdownMigration,
	pallet_cf_validator::Pallet<Runtime>,
	<Runtime as frame_system::Config>::DbWeight,
>;

/// An independent validator counts as active if it has sent a heartbeat within this window.
/// It is far longer than the heartbeat interval so that a node that is briefly offline when
/// the upgrade lands is not removed.
pub const INACTIVITY_THRESHOLD_BLOCKS: BlockNumber = 5 * 24 * 60 * 60 / SECONDS_PER_BLOCK as u32; //5 days

/// Validators with no operator that haven't sent a heartbeat within the threshold. Key holders
/// are excluded: removing them would break signing with the keys they hold.
pub fn inactive_independent_validators() -> Vec<AccountId> {
	let now = frame_system::Pallet::<Runtime>::block_number();
	pallet_cf_account_roles::AccountRoles::<Runtime>::iter()
		.filter(|(_, role)| *role == AccountRole::Validator)
		.map(|(account_id, _)| account_id)
		.filter(|account_id| {
			!pallet_cf_validator::OperatorChoice::<Runtime>::contains_key(account_id) &&
				!pallet_cf_validator::EpochHistory::<Runtime>::is_keyholder(account_id) &&
				pallet_cf_reputation::LastHeartbeat::<Runtime>::get(account_id)
					.is_none_or(|last| now.saturating_sub(last) > INACTIVITY_THRESHOLD_BLOCKS)
		})
		.collect()
}

pub struct NetworkLockdownMigration;

impl UncheckedOnRuntimeUpgrade for NetworkLockdownMigration {
	fn on_runtime_upgrade() -> Weight {
		pallet_cf_validator::NetworkLockdown::<Runtime>::put(true);

		let targets = inactive_independent_validators();
		let (mut deregistered, mut peer_only) = (0u32, 0u32);
		for account_id in &targets {
			match pallet_cf_validator::Pallet::<Runtime>::force_deregister_validator(account_id) {
				Ok(()) => deregistered += 1,
				Err(error) => {
					// Removing the peer mapping is what keeps the node off the p2p network, so
					// fall back to that if any other deregistration check fails.
					log::warn!(
						"Network lockdown: could not deregister validator {account_id:?} ({error:?}); revoking its peer mapping only."
					);
					if pallet_cf_validator::Pallet::<Runtime>::revoke_peer_mapping(account_id) {
						peer_only += 1;
					}
				},
			}
		}
		log::info!(
			"Network lockdown: enabled. Deregistered {deregistered} inactive independent validators; revoked only the peer mapping of {peer_only} more."
		);

		// Deregistration touches a few dozen storage items per account across several pallets.
		let db = <Runtime as frame_system::Config>::DbWeight::get();
		db.reads_writes(50, 50).saturating_mul(targets.len() as u64).saturating_add(
			db.reads(pallet_cf_account_roles::AccountRoles::<Runtime>::iter_keys().count() as u64),
		)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let targets = inactive_independent_validators();
		log::info!("Network lockdown: {} validators to remove.", targets.len());
		Ok(targets.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let targets = Vec::<AccountId>::decode(&mut &state[..])
			.map_err(|_| DispatchError::Other("Failed to decode pre-upgrade state"))?;
		frame_support::ensure!(
			pallet_cf_validator::NetworkLockdown::<Runtime>::get(),
			DispatchError::Other("Network lockdown was not enabled")
		);
		for account_id in &targets {
			frame_support::ensure!(
				!pallet_cf_validator::AccountPeerMapping::<Runtime>::contains_key(account_id),
				DispatchError::Other("An inactive independent validator kept its peer mapping")
			);
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cf_primitives::FlipBalance;
	use cf_traits::{AccountRoleRegistry, Funding, FundingInfo};
	use frame_support::traits::OnRuntimeUpgrade;
	use sp_core::ed25519;

	const NOW: BlockNumber = 1_000_000;
	const STAKE: FlipBalance = 1_000_000_000_000_000_000_000;

	fn validator(seed: u8, peer: u8) -> AccountId {
		let account_id = AccountId::from([seed; 32]);
		frame_system::Pallet::<Runtime>::inc_providers(&account_id);
		// Funds are credited out of the off-chain total, which starts at zero in empty storage.
		pallet_cf_flip::OffchainFunds::<Runtime>::mutate(|funds| *funds += STAKE);
		assert_eq!(
			<pallet_cf_flip::Pallet<Runtime> as Funding>::credit_funds(&account_id, STAKE),
			STAKE
		);
		pallet_cf_account_roles::AccountRoles::<Runtime>::insert(
			&account_id,
			AccountRole::Unregistered,
		);
		assert!(
			pallet_cf_account_roles::Pallet::<Runtime>::register_as_validator(&account_id).is_ok()
		);
		pallet_cf_validator::AccountPeerMapping::<Runtime>::insert(
			&account_id,
			(ed25519::Public::from_raw([peer; 32]), 8078, 1u128),
		);
		pallet_cf_validator::MappedPeers::<Runtime>::insert(
			ed25519::Public::from_raw([peer; 32]),
			(),
		);
		account_id
	}

	fn heartbeat(account_id: &AccountId, at: BlockNumber) {
		pallet_cf_reputation::LastHeartbeat::<Runtime>::insert(account_id, at);
	}

	#[test]
	fn removes_only_inactive_independent_validators() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			frame_system::Pallet::<Runtime>::set_block_number(NOW);

			let active_independent = validator(1, 1);
			heartbeat(&active_independent, NOW - 100);

			let inactive_independent = validator(2, 2);
			heartbeat(&inactive_independent, NOW - INACTIVITY_THRESHOLD_BLOCKS - 1);

			let never_heartbeat = validator(3, 3);
			pallet_cf_validator::ActiveBidder::<Runtime>::mutate(|bidders| {
				bidders.insert(never_heartbeat.clone())
			});

			let inactive_managed = validator(4, 4);
			pallet_cf_validator::OperatorChoice::<Runtime>::insert(
				&inactive_managed,
				AccountId::from([99; 32]),
			);

			let inactive_key_holder = validator(5, 5);
			pallet_cf_validator::HistoricalActiveEpochs::<Runtime>::insert(
				&inactive_key_holder,
				vec![(1, 0)],
			);

			NetworkLockdownMigration::on_runtime_upgrade();

			assert!(pallet_cf_validator::NetworkLockdown::<Runtime>::get());

			for removed in [&inactive_independent, &never_heartbeat] {
				assert!(!pallet_cf_validator::AccountPeerMapping::<Runtime>::contains_key(removed));
				assert!(!pallet_cf_account_roles::Pallet::<Runtime>::has_account_role(
					removed,
					AccountRole::Validator
				));
				// Funds stay in the account.
				assert_eq!(
					<pallet_cf_flip::Pallet<Runtime> as FundingInfo>::total_balance_of(removed),
					STAKE
				);
			}
			assert!(!pallet_cf_validator::ActiveBidder::<Runtime>::get().contains(&never_heartbeat));
			assert!(!pallet_cf_validator::MappedPeers::<Runtime>::contains_key(
				ed25519::Public::from_raw([2; 32])
			));

			for kept in [&active_independent, &inactive_managed, &inactive_key_holder] {
				assert!(pallet_cf_validator::AccountPeerMapping::<Runtime>::contains_key(kept));
				assert!(pallet_cf_account_roles::Pallet::<Runtime>::has_account_role(
					kept,
					AccountRole::Validator
				));
			}
		});
	}

	#[test]
	fn versioned_migration_only_runs_once() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			use frame_support::traits::{GetStorageVersion, StorageVersion};
			frame_system::Pallet::<Runtime>::set_block_number(NOW);
			StorageVersion::new(10).put::<pallet_cf_validator::Pallet<Runtime>>();

			Migration::on_runtime_upgrade();
			assert!(pallet_cf_validator::NetworkLockdown::<Runtime>::get());
			assert_eq!(
				pallet_cf_validator::Pallet::<Runtime>::on_chain_storage_version(),
				StorageVersion::new(11)
			);

			// Governance lifts the lockdown; a later upgrade must not re-enable it.
			pallet_cf_validator::NetworkLockdown::<Runtime>::put(false);
			Migration::on_runtime_upgrade();
			assert!(!pallet_cf_validator::NetworkLockdown::<Runtime>::get());
		});
	}
}
