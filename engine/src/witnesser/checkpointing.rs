use std::sync::Arc;

use anyhow::{Context, Result};
use cf_primitives::EpochIndex;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::multisig::{ChainTag, PersistentKeyDB};

use super::HasChainTag;

mod migrations;

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct WitnessedUntil {
	// epoch_index must be the first element because of `Ord`
	pub epoch_index: EpochIndex,
	pub block_number: u64,
}

pub enum StartCheckpointing<Chain: cf_chains::Chain> {
	Started((Chain::ChainBlockNumber, tokio::sync::mpsc::Sender<WitnessedUntil>)),
	AlreadyWitnessedEpoch,
}

/// Loads the checkpoint from the db then starts checkpointing. Returns the block number at which to
/// start witnessing unless the epoch has already been witnessed.
pub async fn get_witnesser_start_block_with_checkpointing<Chain: cf_chains::Chain + HasChainTag>(
	epoch_start_index: EpochIndex,
	epoch_start_block_number: Chain::ChainBlockNumber,
	db: Arc<PersistentKeyDB>,
) -> Result<StartCheckpointing<Chain>>
where
	<<Chain as cf_chains::Chain>::ChainBlockNumber as TryFrom<u64>>::Error: std::fmt::Debug,
{
	let chain_tag = Chain::CHAIN_TAG;
	let mut loaded_checkpoint = db.load_checkpoint(chain_tag)?;

	// Eth witnessers are the only ones that used the legacy checkpointing files.
	// Only go ahead with the migration if no checkpoint is found in the db.
	if matches!(chain_tag, ChainTag::Ethereum) && loaded_checkpoint.is_none() {
		migrations::run_eth_migration(chain_tag, db.clone(), &std::env::current_dir().unwrap())
			.await
			.with_context(|| "Failed to perform Eth witnesser checkpointing migration")?;
		loaded_checkpoint = db.load_checkpoint(chain_tag)?;
	}

	// Use the loaded checkpoint or the default if none was found
	let witnessed_until = match loaded_checkpoint {
		Some(checkpoint) => {
			info!(
				"Previous {chain_tag} witnesser instance witnessed until epoch {}, block {}",
				checkpoint.epoch_index, checkpoint.block_number
			);
			checkpoint
		},
		None => {
			info!(
				"No {chain_tag} witnesser checkpoint found, using default of {:?}",
				WitnessedUntil::default()
			);
			WitnessedUntil::default()
		},
	};

	// Don't witness epochs that we've already witnessed
	if epoch_start_index < witnessed_until.epoch_index {
		return Ok(StartCheckpointing::AlreadyWitnessedEpoch)
	}

	let (witnessed_until_sender, mut witnessed_until_receiver) = tokio::sync::mpsc::channel(10);

	let mut prev_witnessed_until = witnessed_until.clone();

	tokio::spawn(async move {
		while let Some(new_witnessed_until) = witnessed_until_receiver.recv().await {
			assert!(
				new_witnessed_until > prev_witnessed_until,
				"Expected {new_witnessed_until:?} > {prev_witnessed_until:?}."
			);
			db.update_checkpoint(chain_tag, &new_witnessed_until);
			prev_witnessed_until = new_witnessed_until;
		}
	});

	let start_witnessing_from_block = if witnessed_until.epoch_index == epoch_start_index {
		witnessed_until
			.block_number
			.saturating_add(1)
			.try_into()
			.expect("Should convert block number from u64")
	} else {
		// We haven't started witnessing this epoch yet, so start from the beginning
		// (Note that we do this even if we have already witnessed a few blocks ahead,
		// as we need to re-witness them for the correct epoch)
		epoch_start_block_number
	};

	Ok(StartCheckpointing::Started((start_witnessing_from_block, witnessed_until_sender)))
}

#[cfg(test)]
mod tests {
	use super::*;

	/// This test covers:
	/// - loading a checkpoint from the db
	/// - saving a checkpoint to the db
	/// - sending an new witness causes the checkpoint to be saved to db
	/// - the witnesser start block is the checkpoint +1 if the epoch is the same
	/// - The checkpointing task panics if send a witness of the same block twice
	#[tokio::test(start_paused = true)]
	async fn test_checkpointing() {
		let (_dir, db_path) = utils::testing::new_temp_directory_with_nonexistent_file();

		let saved_witnessed_until = WitnessedUntil { epoch_index: 1, block_number: 2 };
		let expected_witnesser_start = WitnessedUntil {
			epoch_index: saved_witnessed_until.epoch_index,
			block_number: saved_witnessed_until.block_number + 1,
		};

		{
			// Write the starting checkpoint to the db
			let db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();
			db.update_checkpoint(ChainTag::Ethereum, &saved_witnessed_until)
		}

		{
			let db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();

			// Start checkpointing at the same epoch but smaller block number
			match get_witnesser_start_block_with_checkpointing::<cf_chains::Ethereum>(
				saved_witnessed_until.epoch_index,
				saved_witnessed_until
					.block_number
					.checked_sub(1)
					.expect("saved_witnessed_until block number must be larger than 0 for test"),
				Arc::new(db),
			)
			.await
			.unwrap()
			{
				StartCheckpointing::AlreadyWitnessedEpoch => panic!(
					"Should not return `AlreadyWitnessedEpoch` if we start at the same epoch"
				),

				StartCheckpointing::Started((start_witnessing_at, witnessed_until_sender)) => {
					// The checkpointing should tell us to start witnessing at the start block + 1
					assert_eq!(start_witnessing_at, expected_witnesser_start.block_number);

					// Send the first witness at the block it told us to start at and then wait for
					// the checkpointing to update
					witnessed_until_sender.send(expected_witnesser_start.clone()).await.unwrap();

					// Check that the checkpointing task did not crash
					assert!(!witnessed_until_sender.is_closed());

					// Send the same witness again and wait for the checkpointing to update
					witnessed_until_sender
						.send(WitnessedUntil {
							epoch_index: saved_witnessed_until.epoch_index,
							block_number: start_witnessing_at,
						})
						.await
						.unwrap();

					// Give the process time to panic
					tokio::time::sleep(std::time::Duration::from_secs(4)).await;

					// The checkpointing task should have panicked because we should never witness
					// the same block twice
					assert!(witnessed_until_sender.is_closed());
				},
			}
		}

		{
			let db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();

			// The checkpoint in the db should be updated to the expected_witnesser_start
			assert_eq!(
				db.load_checkpoint(ChainTag::Ethereum).unwrap(),
				Some(expected_witnesser_start)
			);
		}
	}

	#[tokio::test]
	async fn should_return_already_witnessed() {
		let (_dir, db_path) = utils::testing::new_temp_directory_with_nonexistent_file();

		let saved_witnessed_until = WitnessedUntil { epoch_index: 2, block_number: 2 };

		{
			// Write the starting checkpoint to the db
			let db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();
			db.update_checkpoint(ChainTag::Ethereum, &saved_witnessed_until)
		}

		let db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();

		// Start checkpointing at a smaller epoch and check that it returns `AlreadyWitnessedEpoch`
		assert!(matches!(
			get_witnesser_start_block_with_checkpointing::<cf_chains::Ethereum>(
				saved_witnessed_until
					.epoch_index
					.checked_sub(1)
					.expect("saved_witnessed_until epoch index must be larger than 0 for test"),
				saved_witnessed_until.block_number,
				Arc::new(db),
			)
			.await
			.unwrap(),
			StartCheckpointing::AlreadyWitnessedEpoch
		));
	}

	#[test]
	fn test_witnessed_until_ord() {
		assert!(
			WitnessedUntil { epoch_index: 2, block_number: 9 } >
				WitnessedUntil { epoch_index: 1, block_number: 10 }
		);
		assert!(
			WitnessedUntil { epoch_index: 2, block_number: 11 } >
				WitnessedUntil { epoch_index: 2, block_number: 10 }
		);
		assert!(
			WitnessedUntil { epoch_index: 2, block_number: 11 } >
				WitnessedUntil { epoch_index: 1, block_number: 10 }
		);
		assert!(
			WitnessedUntil { epoch_index: 1, block_number: 1 } ==
				WitnessedUntil { epoch_index: 1, block_number: 1 }
		);
	}
}
