use std::{
	path::{Path, PathBuf},
	sync::Arc,
};

use anyhow::{Context, Result};
use itertools::Itertools;

use crate::multisig::{ChainTag, PersistentKeyDB};

use super::WitnessedUntil;

const LEGACY_FILE_NAMES: [&str; 2] = ["StakeManager", "KeyManager"];

/// Attempt to migrate from legacy witnesser checkpointing files to db if no checkpoint is found in
/// the db
pub async fn run_migrations(
	chain_tag: ChainTag,
	db: Arc<PersistentKeyDB>,
	legacy_files_path: &Path,
	logger: &slog::Logger,
) -> Result<()> {
	// Eth witnessers are the only ones that used the legacy checkpointing files.
	// Only go ahead with the migration if no checkpoint is found in the db.
	if matches!(chain_tag, ChainTag::Ethereum) && db.load_checkpoint(chain_tag)?.is_none() {
		// Check for a legacy checkpoint and save it to the db
		if let Some(legacy_witness_until) =
			futures::future::join_all(LEGACY_FILE_NAMES.iter().map(|name| {
				Box::pin(load_from_legacy_checkpoint_file(legacy_files_path.join(name), logger))
			}))
			.await
			.into_iter()
			.collect::<Option<Vec<WitnessedUntil>>>()
			.and_then(|checkpoints| {
				checkpoints
					.into_iter()
					.sorted_by_key(|witnessed_until| witnessed_until.block_number)
					.next()
			}) {
			slog::info!(
				logger,
				"Migrating legacy witnesser {chain_tag} checkpoint of {:?} to db",
				legacy_witness_until
			);
			db.update_checkpoint(chain_tag, &legacy_witness_until);

			if let Err(e) = delete_legacy_checkpointing_files(legacy_files_path) {
				slog::error!(logger, "Failed to delete legacy checkpointing files: {e}");
			}
		}
	}

	Ok(())
}

async fn load_from_legacy_checkpoint_file(
	file_path: PathBuf,
	logger: &slog::Logger,
) -> Option<WitnessedUntil> {
	if file_path.exists() {
		tokio::task::spawn_blocking({
			let file_path = file_path.clone();
			let logger = logger.clone();
			move || {
				std::fs::read_to_string(file_path)
					.map_err(anyhow::Error::new)
					.and_then(|string| {
					serde_json::from_str::<WitnessedUntil>(&string).map_err(anyhow::Error::new)
					})
					.map_err(|e| {
						slog::error!(
							logger,
							"Failed to read legacy witnesser checkpoint file: {e}"
						);
						e
					})
					.ok()
			}
		})
		.await
		.unwrap()
	} else {
		None
	}
}

fn delete_legacy_checkpointing_files(legacy_files_path: &Path) -> Result<()> {
	for name in LEGACY_FILE_NAMES {
		let file_path = legacy_files_path.join(name);
		if file_path.exists() {
			std::fs::remove_file(file_path)?;
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use std::{collections::HashMap, io::Write};

	use utilities::assert_ok;

	use crate::{
		logging::test_utils::new_test_logger, testing::new_temp_directory_with_nonexistent_file,
	};

	use super::*;

	fn write_legacy_checkpoint(file_path: PathBuf, witnessed_until: WitnessedUntil) {
		atomicwrites::AtomicFile::new(file_path, atomicwrites::OverwriteBehavior::AllowOverwrite)
			.write(|file| {
				write!(
					file,
					"{}",
					serde_json::to_string::<WitnessedUntil>(&witnessed_until).unwrap()
				)
			})
			.unwrap();
	}

	#[tokio::test]
	async fn should_migrate_legacy_checkpoint_to_db() {
		let logger = new_test_logger();
		let (temp_dir, db_path) = new_temp_directory_with_nonexistent_file();
		let temp_path = temp_dir.path().to_owned();

		// Checkpoints to save to the legacy files
		let expected_witness_until_list: HashMap<&str, WitnessedUntil> = HashMap::from_iter(vec![
			(LEGACY_FILE_NAMES[0], WitnessedUntil { epoch_index: 1, block_number: 6 }),
			// Has the lowest block number
			(LEGACY_FILE_NAMES[1], WitnessedUntil { epoch_index: 9, block_number: 1 }),
		]);

		// Create both witnesser legacy checkpointing files
		for (name, witness_until) in expected_witness_until_list.clone() {
			let file_path = temp_path.join(name);
			write_legacy_checkpoint(file_path, witness_until);
		}

		// Run the migration
		{
			let db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();
			assert_ok!(run_migrations(ChainTag::Ethereum, Arc::new(db), &temp_path, &logger).await);
		}

		// Load the checkpoint from the db and make sure it is the one with the lowest
		// block number
		let db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();
		let witnessed_until = db
			.load_checkpoint(ChainTag::Ethereum)
			.unwrap()
			.expect("Migration should have saved to db");
		assert_eq!(witnessed_until.block_number, 1);
		assert_eq!(witnessed_until.epoch_index, 9);

		// Check that the legacy files were deleted (with a small delay to allow for file delete)
		std::thread::sleep(std::time::Duration::from_millis(50));
		for name in LEGACY_FILE_NAMES {
			assert!(!temp_path.join(name).exists());
		}
	}
}
