use std::{sync::Arc, time::Duration};

use cf_primitives::EpochIndex;
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

use crate::multisig::{ChainTag, PersistentKeyDB};

const UPDATE_INTERVAL: Duration = Duration::from_secs(4);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct WitnessedUntil {
	pub epoch_index: EpochIndex,
	pub block_number: u64,
}

pub fn start_checkpointing_for(
	chain_tag: ChainTag,
	db: Arc<PersistentKeyDB>,
	logger: &slog::Logger,
) -> (WitnessedUntil, tokio::sync::watch::Sender<WitnessedUntil>, JoinHandle<()>) {
	// Load the checkpoint or use the default if none is found
	let witnessed_until = match db.load_checkpoint(chain_tag) {
		Ok(Some(checkpoint)) => {
			slog::info!(
				logger,
				"Previous {chain_tag} witnesser instance witnessed until epoch {}, block {}",
				checkpoint.epoch_index,
				checkpoint.block_number
			);
			checkpoint
		},
		Ok(None) => {
			slog::info!(
				logger,
				"No {chain_tag} witnesser checkpoint found, using default of {:?}",
				WitnessedUntil::default()
			);
			WitnessedUntil::default()
		},
		Err(e) => {
			slog::error!(
				logger,
				"Failed to load {chain_tag} witnesser checkpoint, using default of {:?}: {e}",
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
					db.update_checkpoint(chain_tag, &changed_witnessed_until);
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
	use crate::logging::test_utils::new_test_logger;

	#[tokio::test(start_paused = true)]
	async fn should_save_and_load_checkpoint() {
		let logger = new_test_logger();

		let updated_witnessed_until = WitnessedUntil { epoch_index: 1, block_number: 2 };
		assert_ne!(updated_witnessed_until, WitnessedUntil::default());

		let (_dir, db_path) = crate::testing::new_temp_directory_with_nonexistent_file();

		{
			// Start checkpointing in a fresh database
			let db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();

			let (witnessed_until, witnessed_until_sender, checkpointing_join_handle) =
				start_checkpointing_for(ChainTag::Ethereum, Arc::new(db), &logger);
			assert_eq!(witnessed_until, WitnessedUntil::default());

			// Send an updated checkpoint to be saved to the db
			assert_ok!(witnessed_until_sender.send(updated_witnessed_until.clone()));

			// Wait for longer than the update interval to ensure the update is processed.
			tokio::time::sleep(UPDATE_INTERVAL * 2).await;

			// Dropping the sender causes the task to complete.
			drop(witnessed_until_sender);
			checkpointing_join_handle.await.unwrap();
		}

		{
			// Start checkpointing again with the same db file
			let db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();
			let (witnessed_until, _, _) =
				start_checkpointing_for(ChainTag::Ethereum, Arc::new(db), &logger);

			// The checkpoint should be the updated value that was saved in the db
			assert_eq!(witnessed_until, updated_witnessed_until);
		}
	}
}
