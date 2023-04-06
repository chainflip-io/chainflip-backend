use std::sync::Arc;

use futures::{FutureExt, Stream, StreamExt};
use num_traits::CheckedSub;
use std::pin::Pin;
use tokio::{
	select,
	sync::{oneshot, Mutex},
};

use async_trait::async_trait;
use tracing::{error, info};

use crate::task_scope::{task_scope, ScopedJoinHandle};

use super::{ChainBlockNumber, EpochStart};

#[async_trait]
pub trait EpochWitnesser: Send + Sync + 'static {
	type Chain: cf_chains::Chain;
	/// Chunk of data to process in each call to [Self::do_witness]
	type Data;
	/// State that persists across epochs
	type StaticState: Send;

	async fn do_witness(
		&mut self,
		data: Self::Data,
		state: &mut Self::StaticState,
	) -> anyhow::Result<()>;

	/// Whether the witnesser has any more processing to do for the current epoch
	fn should_finish(&self, last_block_number_for_epoch: ChainBlockNumber<Self::Chain>) -> bool;
}

pub type WitnesserAndStream<W> =
	(W, Pin<Box<dyn Stream<Item = anyhow::Result<<W as EpochWitnesser>::Data>> + Send + 'static>>);

#[async_trait]
pub trait EpochWitnesserGenerator<W: EpochWitnesser>: Send {
	async fn init(
		&mut self,
		epoch: EpochStart<W::Chain>,
	) -> anyhow::Result<Option<WitnesserAndStream<W>>>;

	fn should_process_historical_epochs() -> bool;
}

pub async fn start_epoch_witnesser<Witnesser, Generator>(
	epoch_start_receiver: Arc<Mutex<async_broadcast::Receiver<EpochStart<Witnesser::Chain>>>>,
	mut witnesser_generator: Generator,
	initial_state: Witnesser::StaticState,
) -> Result<(), ()>
where
	Witnesser: EpochWitnesser,
	Generator: EpochWitnesserGenerator<Witnesser>,
	Witnesser::Data: Send + 'static,
{
	task_scope(|scope| {
		async {
			info!("Starting");

			let mut option_state = Some(initial_state);
			let mut current_task: Option<(
				oneshot::Sender<ChainBlockNumber<Witnesser::Chain>>,
				ScopedJoinHandle<Witnesser::StaticState>,
			)> = None;

			let mut epoch_start_receiver =
				epoch_start_receiver.try_lock().expect("should have exclusive ownership");

			loop {
				let epoch_start = epoch_start_receiver.recv().await.expect("Sender closed");

				if let Some((end_witnessing_sender, handle)) = current_task.take() {
					// Send a signal to the previous epoch to stop at the starting block of the new
					// epoch
					let last_block_number_in_epoch = epoch_start
						.block_number
						.checked_sub(&ChainBlockNumber::<Witnesser::Chain>::from(1u32))
						.expect("only the first epoch can start from 0");
					end_witnessing_sender.send(last_block_number_in_epoch).unwrap();

					assert!(
						option_state.replace(handle.await).is_none(),
						"state must have been consumed by generator"
					);
				}

				if !epoch_start.participant ||
					(!epoch_start.current && !Generator::should_process_historical_epochs())
				{
					continue
				}

				info!("Start witnessing from block: {}", epoch_start.block_number);

				let (end_witnessing_sender, end_witnessing_receiver) = oneshot::channel();

				if let Some((witnesser, data_stream)) =
					witnesser_generator.init(epoch_start).await.map_err(|e| {
						error!("Error while initializing epoch witnesser: {:?}", e);
					})? {
					current_task = Some((
						end_witnessing_sender,
						scope.spawn_with_handle(run_witnesser(
							witnesser,
							data_stream,
							end_witnessing_receiver,
							option_state.take().expect("state must be present"),
						)),
					));
				};
			}
		}
		.boxed()
	})
	.await
}

async fn run_witnesser<Witnesser>(
	mut witnesser: Witnesser,
	mut data_stream: std::pin::Pin<
		Box<dyn futures::Stream<Item = anyhow::Result<Witnesser::Data>> + Send + 'static>,
	>,
	end_witnessing_receiver: oneshot::Receiver<ChainBlockNumber<Witnesser::Chain>>,
	mut state: Witnesser::StaticState,
) -> Result<Witnesser::StaticState, ()>
where
	Witnesser: EpochWitnesser,
{
	// If set, this is the last block to process
	let mut last_block_number_for_epoch: Option<ChainBlockNumber<Witnesser::Chain>> = None;

	let mut end_witnessing_receiver = end_witnessing_receiver.fuse();

	loop {
		select! {
			Ok(last_block_number) = &mut end_witnessing_receiver => {

				if witnesser.should_finish(last_block_number) {
					break;
				}
				last_block_number_for_epoch = Some(last_block_number);
			},
			Some(data) = data_stream.next() => {
				// This will be an error if the stream times out. When it does, we return
				// an error so that we restart the witnesser.
				let data = data.map_err(|e| {
					error!("Error while getting data for witnesser: {:?}", e);
				})?;

				witnesser.do_witness(data, &mut state).await.map_err(|_| {
					error!("Witnesser failed to process data")
				})?;

				if let Some(block_number) = last_block_number_for_epoch {
					if witnesser.should_finish(block_number) {
						break;
					}
				}
			},
		}
	}

	info!("Epoch witnesser finished epoch");

	Ok(state)
}
}
