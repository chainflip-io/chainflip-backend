use std::{io::Write, time::Duration};

use cf_primitives::EpochIndex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WitnessedUntil {
	pub epoch_index: EpochIndex,
	pub block_number: u64,
}

pub async fn start_checkpointing_for(
	witnesser_name: &str,
	logger: &slog::Logger,
) -> (WitnessedUntil, tokio::sync::watch::Sender<WitnessedUntil>) {
	let mut file_path = std::env::current_dir().unwrap();
	file_path.push(witnesser_name);

	let witnessed_until = tokio::task::spawn_blocking({
		let file_path = file_path.clone();
		move || match std::fs::read_to_string(&file_path).map_err(anyhow::Error::new).and_then(
			|string| serde_json::from_str::<WitnessedUntil>(&string).map_err(anyhow::Error::new),
		) {
			Ok(witnessed_record) => witnessed_record,
			Err(_) => WitnessedUntil { epoch_index: 0, block_number: 0 },
		}
	})
	.await
	.unwrap();

	slog::info!(
		logger,
		"Previous {witnesser_name} witnesser instance witnessed until epoch {}, block {}",
		witnessed_until.epoch_index,
		witnessed_until.block_number
	);

	let (witnessed_until_sender, witnessed_until_receiver) =
		tokio::sync::watch::channel(witnessed_until.clone());

	// check if witnessed until has changed; write to a file if so
	tokio::task::spawn_blocking({
		let file_path = file_path.clone();
		let logger = logger.clone();
		move || loop {
			std::thread::sleep(Duration::from_secs(4));
			if let Ok(changed) = witnessed_until_receiver.has_changed() {
				if changed {
					let witnessed_until = witnessed_until_receiver.borrow().clone();

					if let Err(error) = atomicwrites::AtomicFile::new(
						&file_path,
						atomicwrites::OverwriteBehavior::AllowOverwrite,
					)
					.write(|file| {
						write!(
							file,
							"{}",
							serde_json::to_string::<WitnessedUntil>(&witnessed_until).unwrap()
						)
					}) {
						slog::info!(logger, "Failed to record WitnessingUntil: {:?}", error);
					} else {
						slog::info!(logger, "Recorded WitnessingUntil: {:?}", witnessed_until);
					}
				}
			} else {
				break
			}
		}
	});

	(witnessed_until, witnessed_until_sender)
}
