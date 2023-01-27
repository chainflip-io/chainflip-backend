use std::{sync::Arc, time::Duration};

use cf_primitives::EpochIndex;
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

use crate::multisig::{CryptoScheme, PersistentKeyDB};

const UPDATE_INTERVAL: Duration = Duration::from_secs(4);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct WitnessedUntil {
	pub epoch_index: EpochIndex,
	pub block_number: u64,
}

pub fn start_checkpointing_for<C: CryptoScheme>(
	witnesser_name: &str,
	db: Arc<PersistentKeyDB>,
	logger: &slog::Logger,
) -> (WitnessedUntil, tokio::sync::watch::Sender<WitnessedUntil>, JoinHandle<()>) {
	// Load the checkpoint or use the default if none is found
	let witnessed_until = match db.load_checkpoint::<C>() {
		Ok(Some(checkpoint)) => {
			slog::info!(
				logger,
				"Previous {witnesser_name} witnesser instance witnessed until epoch {}, block {}",
				checkpoint.epoch_index,
				checkpoint.block_number
			);
			checkpoint
		},
		Ok(None) => {
			slog::info!(
				logger,
				"No {witnesser_name} witnesser checkpoint found, using default of {:?}",
				WitnessedUntil::default()
			);
			WitnessedUntil::default()
		},
		Err(e) => {
			slog::error!(
				logger,
				"Failed to load {witnesser_name} witnesser checkpoint, using default of {:?}: {e}",
				WitnessedUntil::default()
			);
			WitnessedUntil::default()
		},
	};

	let (witnessed_until_sender, witnessed_until_receiver) =
		tokio::sync::watch::channel(witnessed_until.clone());

	let mut prev_witnessed_until = witnessed_until.clone();

	let join_handle = tokio::spawn(async move {
		// Check every few seconds if the `witnessed_until` has changed and then update the database
		loop {
			tokio::time::sleep(UPDATE_INTERVAL).await;
			if let Ok(changed) = witnessed_until_receiver.has_changed() {
				if changed {
					let changed_witnessed_until = witnessed_until_receiver.borrow().clone();
					assert!(
						changed_witnessed_until.epoch_index > prev_witnessed_until.epoch_index ||
							changed_witnessed_until.block_number >
								prev_witnessed_until.block_number
					);
					db.update_checkpoint::<C>(&changed_witnessed_until);
					prev_witnessed_until = changed_witnessed_until;
				}
			} else {
				break
			}
		}
	});

	(witnessed_until, witnessed_until_sender, join_handle)
}

#[cfg(test)]
mod tests {
	use utilities::assert_ok;

	use super::*;
	use crate::{logging::test_utils::new_test_logger, multisig::eth::EthSigning};

	#[tokio::test]
	async fn should_save_and_load_checkpoint() {
		let logger = new_test_logger();
		type Scheme = EthSigning;

		let updated_witnessed_until = WitnessedUntil { epoch_index: 1, block_number: 2 };
		assert_ne!(updated_witnessed_until, WitnessedUntil::default());

		let (_dir, db_path) = crate::testing::new_temp_directory_with_nonexistent_file();

		{
			// Start checkpointing in a fresh database
			let db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();

			let (witnessed_until, witnessed_until_sender, checkpointing_join_handle) =
				start_checkpointing_for::<Scheme>("test1", Arc::new(db), &logger);
			assert_eq!(witnessed_until, WitnessedUntil::default());

			// Send an updated checkpoint to be saved to the db
			assert_ok!(witnessed_until_sender.send(updated_witnessed_until.clone()));

			// Skip some time so that the update goes through
			tokio::time::sleep(Duration::from_millis(50)).await;
			tokio::time::pause();
			tokio::time::advance(UPDATE_INTERVAL).await;
			tokio::time::resume();
			tokio::time::sleep(Duration::from_millis(50)).await;

			// Abort the task so we can open the db file again later
			checkpointing_join_handle.abort();

			// Skip some time to wait for the task to wake up and abort
			tokio::time::pause();
			tokio::time::advance(UPDATE_INTERVAL).await;
			tokio::time::resume();
			assert!(checkpointing_join_handle.is_finished());
		}

		{
			// Start checkpointing again with the same db file
			let db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();
			let (witnessed_until, _, _) =
				start_checkpointing_for::<Scheme>("test2", Arc::new(db), &logger);

			// The checkpoint should be the updated value that was saved in the db
			assert_eq!(witnessed_until, updated_witnessed_until);
		}
	}
}
