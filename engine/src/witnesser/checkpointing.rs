use std::{sync::Arc, time::Duration};

use cf_primitives::EpochIndex;
use serde::{Deserialize, Serialize};

use crate::multisig::{ChainTag, PersistentKeyDB};

const UPDATE_INTERVAL: Duration = Duration::from_secs(4);

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct WitnessedUntil {
	pub epoch_index: EpochIndex,
	pub block_number: u64,
}

pub enum StartCheckpointing<Chain: cf_chains::Chain> {
	Started((Chain::ChainBlockNumber, tokio::sync::watch::Sender<WitnessedUntil>)),
	AlreadyWitnessedEpoch,
}

/// Loads the checkpoint from the db then starts checkpointing. Returns the block number at which to
/// start witnessing unless the epoch has already been witnessed.
pub fn get_witnesser_start_block_with_checkpointing<Chain: cf_chains::Chain>(
	chain_tag: ChainTag,
	epoch_start_index: EpochIndex,
	epoch_start_block_number: Chain::ChainBlockNumber,
	db: Arc<PersistentKeyDB>,
	logger: &slog::Logger,
) -> StartCheckpointing<Chain>
where
	<<Chain as cf_chains::Chain>::ChainBlockNumber as TryFrom<u64>>::Error: std::fmt::Debug,
{
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

	// Don't witness epochs that we've already witnessed
	if epoch_start_index < witnessed_until.epoch_index {
		return StartCheckpointing::AlreadyWitnessedEpoch
	}

	let (witnessed_until_sender, witnessed_until_receiver) =
		tokio::sync::watch::channel(witnessed_until.clone());

	let mut prev_witnessed_until = witnessed_until.clone();

	tokio::spawn(async move {
		// Check every few seconds if the `witnessed_until` has changed and then update the database
		loop {
			tokio::time::sleep(UPDATE_INTERVAL).await;
			if let Ok(changed) = witnessed_until_receiver.has_changed() {
				if changed {
					let changed_witnessed_until = witnessed_until_receiver.borrow().clone();
					assert!(
						changed_witnessed_until > prev_witnessed_until,
						"Expected {changed_witnessed_until:?} > {prev_witnessed_until:?}."
					);
					db.update_checkpoint(chain_tag, &changed_witnessed_until);
					prev_witnessed_until = changed_witnessed_until;
				}
			} else {
				break
			}
		}
	});

	// We do this because it's possible to witness ahead of the epoch start during the
	// previous epoch. If we don't start witnessing from the epoch start, when we
	// receive a new epoch, we won't witness some of the blocks for the particular
	// epoch, since witness extrinsics are submitted with the epoch number it's for.
	let start_witnessing_from_block = if witnessed_until.epoch_index == epoch_start_index {
		witnessed_until
			.block_number
			.saturating_add(1)
			.try_into()
			.expect("Should convert block number from u64")
	} else {
		// We haven't started witnessing this epoch yet, so start from the beginning
		epoch_start_block_number
	};

	StartCheckpointing::Started((start_witnessing_from_block, witnessed_until_sender))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::logging::test_utils::new_test_logger;

	/// This test covers:
	/// - loading a checkpoint from the db
	/// - saving a checkpoint to the db
	/// - sending an new witness causes the checkpoint to be saved to db
	/// - the witnesser start block is the checkpoint +1 if the epoch is the same
	/// - The checkpointing task panics if send a witness of the same block twice
	#[tokio::test(start_paused = true)]
	async fn test_checkpointing() {
		let logger = new_test_logger();
		let (_dir, db_path) = crate::testing::new_temp_directory_with_nonexistent_file();

		let saved_witnessed_until = WitnessedUntil { epoch_index: 1, block_number: 2 };
		let expected_witnesser_start = WitnessedUntil {
			epoch_index: saved_witnessed_until.epoch_index,
			block_number: saved_witnessed_until.block_number + 1,
		};

		{
			// Write the starting checkpoint to the db
			let db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();
			db.update_checkpoint(ChainTag::Ethereum, &saved_witnessed_until)
		}

		{
			let db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();

			// Start checkpointing at the same epoch but smaller block number
			match get_witnesser_start_block_with_checkpointing::<cf_chains::Ethereum>(
				ChainTag::Ethereum,
				saved_witnessed_until.epoch_index,
				saved_witnessed_until
					.block_number
					.checked_sub(1)
					.expect("saved_witnessed_until block number must be larger than 0 for test"),
				Arc::new(db),
				&logger,
			) {
				StartCheckpointing::AlreadyWitnessedEpoch => panic!(
					"Should not return `AlreadyWitnessedEpoch` if we start at the same epoch"
				),

				StartCheckpointing::Started((start_witnessing_at, witnessed_until_sender)) => {
					// The checkpointing should tell us to start witnessing at the start block + 1
					assert_eq!(start_witnessing_at, expected_witnesser_start.block_number);

					// Send the first witness at the block it told us to start at and then wait for
					// the checkpointing to update
					witnessed_until_sender.send(expected_witnesser_start.clone()).unwrap();
					tokio::time::sleep(UPDATE_INTERVAL * 2).await;

					// Check that the checkpointing task did not crash
					assert!(!witnessed_until_sender.is_closed());

					// Send the same witness again and wait for the checkpointing to update
					witnessed_until_sender
						.send(WitnessedUntil {
							epoch_index: saved_witnessed_until.epoch_index,
							block_number: start_witnessing_at,
						})
						.unwrap();
					tokio::time::sleep(UPDATE_INTERVAL * 2).await;

					// The checkpointing task should have panicked because we should never witness
					// the same block twice
					assert!(witnessed_until_sender.is_closed());
				},
			}
		}

		{
			let db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();

			// The checkpoint in the db should be updated to the expected_witnesser_start
			assert_eq!(
				db.load_checkpoint(ChainTag::Ethereum).unwrap(),
				Some(expected_witnesser_start)
			);
		}
	}

	#[tokio::test]
	async fn should_return_already_witnessed() {
		let logger = new_test_logger();
		let (_dir, db_path) = crate::testing::new_temp_directory_with_nonexistent_file();

		let saved_witnessed_until = WitnessedUntil { epoch_index: 2, block_number: 2 };

		{
			// Write the starting checkpoint to the db
			let db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();
			db.update_checkpoint(ChainTag::Ethereum, &saved_witnessed_until)
		}

		let db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();

		// Start checkpointing at a smaller epoch and check that it returns `AlreadyWitnessedEpoch`
		assert!(matches!(
			get_witnesser_start_block_with_checkpointing::<cf_chains::Ethereum>(
				ChainTag::Ethereum,
				saved_witnessed_until
					.epoch_index
					.checked_sub(1)
					.expect("saved_witnessed_until epoch index must be larger than 0 for test"),
				saved_witnessed_until.block_number,
				Arc::new(db),
				&logger,
			),
			StartCheckpointing::AlreadyWitnessedEpoch
		));
	}
}
