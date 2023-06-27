use std::{fmt::Display, sync::Arc};

use futures::{FutureExt, Stream, StreamExt};
use futures_util::TryFutureExt;
use num_traits::CheckedSub;
use std::pin::Pin;
use tokio::{
	select,
	sync::{oneshot, Mutex},
};

use async_trait::async_trait;
use tracing::{error, info, warn, Instrument};

use utilities::task_scope::{task_scope, ScopedJoinHandle};

use super::{ChainBlockNumber, EpochStart, HasBlockNumber};
type BlockNumber<Witnesser> = ChainBlockNumber<<Witnesser as EpochWitnesser>::Chain>;

#[async_trait]
pub trait EpochWitnesser: Send + Sync + 'static {
	type Chain: cf_chains::Chain;
	/// Chunk of data to process in each call to [Self::do_witness]
	type Data: Send;
	/// State that persists across epochs
	type StaticState: Send;

	const SHOULD_PROCESS_HISTORICAL_EPOCHS: bool;

	async fn run_witnesser(
		self,
		mut data_stream: std::pin::Pin<
			Box<dyn futures::Stream<Item = anyhow::Result<Self::Data>> + Send + 'static>,
		>,
		end_witnessing_receiver: oneshot::Receiver<BlockNumber<Self>>,
		mut state: Self::StaticState,
	) -> Result<Self::StaticState, ()>;

	async fn do_witness(
		&mut self,
		data: Self::Data,
		state: &mut Self::StaticState,
	) -> anyhow::Result<()>;
}

type StreamToWitness<W> =
	Pin<Box<dyn Stream<Item = anyhow::Result<<W as EpochWitnesser>::Data>> + Send + 'static>>;

pub enum WitnesserInitResult<W: EpochWitnesser> {
	Created((W, StreamToWitness<W>)),
	EpochSkipped,
}

#[async_trait]
pub trait EpochProcessGenerator: Send {
	type Witnesser: EpochWitnesser;

	async fn init(
		&mut self,
		epoch: EpochStart<<Self::Witnesser as EpochWitnesser>::Chain>,
	) -> anyhow::Result<WitnesserInitResult<Self::Witnesser>>;
}

type WitnesserTask<Witnesser> = ScopedJoinHandle<<Witnesser as EpochWitnesser>::StaticState>;

#[derive(Debug)]
pub enum EpochProcessRunnerError<C: cf_chains::Chain> {
	WitnesserError(EpochStart<C>),
	Other(anyhow::Error),
}

impl<C: cf_chains::Chain> Display for EpochProcessRunnerError<C> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			EpochProcessRunnerError::WitnesserError(e) => {
				write!(f, "Epoch processor witnessing error at epoch: {:?}", e)
			},
			EpochProcessRunnerError::Other(e) => write!(f, "Epoch process error: {:?}", e),
		}
	}
}

impl<C: cf_chains::Chain> std::error::Error for EpochProcessRunnerError<C> {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			EpochProcessRunnerError::WitnesserError(_) => None,
			EpochProcessRunnerError::Other(e) => Some(e.as_ref()),
		}
	}
}

impl<C: cf_chains::Chain> From<()> for EpochProcessRunnerError<C> {
	fn from(_: ()) -> Self {
		EpochProcessRunnerError::Other(anyhow::anyhow!("Unknown Error"))
	}
}

pub async fn start_epoch_process_runner<Generator>(
	mut resume_at: Option<EpochStart<<Generator::Witnesser as EpochWitnesser>::Chain>>,
	epoch_start_receiver: Arc<
		Mutex<
			async_broadcast::Receiver<EpochStart<<Generator::Witnesser as EpochWitnesser>::Chain>>,
		>,
	>,
	mut witnesser_generator: Generator,
	initial_state: <Generator::Witnesser as EpochWitnesser>::StaticState,
) -> Result<(), EpochProcessRunnerError<<Generator::Witnesser as EpochWitnesser>::Chain>>
where
	Generator: EpochProcessGenerator,
{
	task_scope(|scope| {
		async {
			info!("Starting");

			let mut option_state = Some(initial_state);
			let mut current_task: Option<(
				oneshot::Sender<BlockNumber<Generator::Witnesser>>,
				WitnesserTask<Generator::Witnesser>,
			)> = None;

			let mut epoch_start_receiver =
				epoch_start_receiver.try_lock().expect("should have exclusive ownership");

			loop {
				let epoch_start = if let Some(epoch_start) = resume_at.take() {
					epoch_start
				} else {
					epoch_start_receiver.recv().await.expect("Sender closed")
				};

				if let Some((end_witnessing_sender, handle)) = current_task.take() {
					// Send a signal to the previous epoch's witnesser process
					// to stop epoch at the starting block of the new epoch
					let last_block_number_in_epoch = epoch_start
						.block_number
						.checked_sub(&BlockNumber::<Generator::Witnesser>::from(1u32))
						.expect("only the first epoch can start from 0");
					end_witnessing_sender.send(last_block_number_in_epoch).unwrap();

					assert!(
						option_state.replace(handle.await).is_none(),
						"state must have been consumed by generator if we have started a new task"
					);
				}

				// Must be a current participant in the epoch to witness. Additionally, only
				// certain witnessers (e.g. deposit) process non-current/historical epochs.
				if epoch_start.participant &&
					(epoch_start.current ||
						<Generator::Witnesser>::SHOULD_PROCESS_HISTORICAL_EPOCHS)
				{
					info!("Start witnessing from block: {}", epoch_start.block_number);

					let (end_witnessing_sender, end_witnessing_receiver) = oneshot::channel();

					if let WitnesserInitResult::Created((witnesser, data_stream)) =
						witnesser_generator.init(epoch_start.clone()).await.map_err(|e| {
							error!("Error while initializing epoch witnesser: {:?}", e);
							EpochProcessRunnerError::Other(e)
						})? {
						current_task = Some((
							end_witnessing_sender,
							scope.spawn_with_handle(
								witnesser
									.run_witnesser(
										data_stream,
										end_witnessing_receiver,
										option_state.take().expect("state must be present"),
									)
									.instrument(tracing::info_span!(
										"EpochWitnesser",
										chain =
											<<Generator::Witnesser as EpochWitnesser>::Chain as cf_chains::Chain>::NAME,
										epoch = &epoch_start.epoch_index,
									))
									.map_err(|_| {
										EpochProcessRunnerError::WitnesserError(epoch_start)
									}),
							),
						));
					};
				}
			}
		}
		.boxed()
	})
	.await
}

pub async fn run_witnesser_data_stream<Witnesser>(
	mut witnesser: Witnesser,
	mut data_stream: std::pin::Pin<
		Box<dyn futures::Stream<Item = anyhow::Result<Witnesser::Data>> + Send + 'static>,
	>,
	end_witnessing_receiver: oneshot::Receiver<BlockNumber<Witnesser>>,
	mut state: Witnesser::StaticState,
) -> Result<Witnesser::StaticState, ()>
where
	Witnesser: EpochWitnesser,
{
	let mut end_witnessing_receiver = end_witnessing_receiver.fuse();

	loop {
		select! {
			Ok(_) = &mut end_witnessing_receiver => {
				break;
			},
			next = data_stream.next() => {
				// This will be an error if the stream times out. When it does, we return
				// an error so that we restart the witnesser
				if let Some(data) = next {
					witnesser.do_witness(
						data.map_err(|e| {
							error!("Error while getting data for witnesser: {:?}", e);
						})?,
						&mut state
					)
					.await
					.map_err(|e| {
						error!("Witnesser failed to process data: {:?}", e);
					})?;
				} else {
					warn!("No more data on witnesser data stream. Exiting.");
					return Err(());
				}
			},
		}
	}

	info!("Epoch witnesser finished epoch");

	Ok(state)
}

fn should_end_witnessing<W: EpochWitnesser>(
	last_processed_block: Option<BlockNumber<W>>,
	last_block_in_epoch: Option<BlockNumber<W>>,
) -> bool {
	match (last_processed_block, last_block_in_epoch) {
		(Some(last_processed_block), Some(last_block_in_epoch)) =>
			last_processed_block >= last_block_in_epoch,
		// We continue witnessing if we don't know when the epoch ends
		// or which blocks we have already processed
		_ => false,
	}
}

pub async fn run_witnesser_block_stream<Witnesser>(
	mut witnesser: Witnesser,
	mut block_stream: std::pin::Pin<
		Box<dyn futures::Stream<Item = anyhow::Result<Witnesser::Data>> + Send + 'static>,
	>,
	end_witnessing_receiver: oneshot::Receiver<BlockNumber<Witnesser>>,
	mut state: Witnesser::StaticState,
) -> Result<Witnesser::StaticState, ()>
where
	Witnesser: EpochWitnesser,
	Witnesser::Data: HasBlockNumber<BlockNumber = BlockNumber<Witnesser>>,
{
	// If set, this is the last block to process
	let mut last_block_number_for_epoch: Option<BlockNumber<Witnesser>> = None;
	let mut last_processed_block = None;

	let mut end_witnessing_receiver = end_witnessing_receiver.fuse();

	loop {
		select! {
			Ok(last_block_number) = &mut end_witnessing_receiver => {
				last_block_number_for_epoch = Some(last_block_number);

				if should_end_witnessing::<Witnesser>(last_processed_block, last_block_number_for_epoch) {
					break;
				}

			},
			Some(block) = block_stream.next().instrument(tracing::debug_span!("Block-Stream-Future")) => {
				// This will be an error if the stream times out. When it does, we return
				// an error so that we restart the witnesser.
				let block = block.map_err(|e| {
					error!("Error while getting block for witnesser: {:?}", e);
				})?;

				let block_number = block.block_number();

				witnesser.do_witness(block, &mut state).await.map_err(|_| {
					error!("Witnesser failed to process block")
				})?;

				last_processed_block = Some(block_number);

				if should_end_witnessing::<Witnesser>(last_processed_block, last_block_number_for_epoch) {
					break;
				}

			},
		}
	}

	info!("Epoch witnesser finished epoch");

	Ok(state)
}

#[cfg(test)]
mod epoch_witnesser_testing {

	use utilities::testing::recv_with_timeout;

	use super::*;

	struct TestEpochWitnesser {
		last_processed_block: u64,
		processed_blocks_sender: tokio::sync::mpsc::UnboundedSender<u64>,
	}

	#[async_trait]
	impl EpochWitnesser for TestEpochWitnesser {
		type Chain = cf_chains::Ethereum;

		type Data = u64;

		type StaticState = ();

		const SHOULD_PROCESS_HISTORICAL_EPOCHS: bool = true;

		async fn run_witnesser(
			self,
			data_stream: std::pin::Pin<
				Box<dyn futures::Stream<Item = anyhow::Result<Self::Data>> + Send + 'static>,
			>,
			end_witnessing_receiver: oneshot::Receiver<BlockNumber<Self>>,
			state: Self::StaticState,
		) -> Result<Self::StaticState, ()> {
			run_witnesser_block_stream(self, data_stream, end_witnessing_receiver, state).await
		}

		async fn do_witness(&mut self, block: u64, _: &mut ()) -> anyhow::Result<()> {
			if block == u64::MAX {
				return Err(anyhow::anyhow!("WTF!"))
			}
			self.last_processed_block = block;
			self.processed_blocks_sender.send(block).unwrap();
			Ok(())
		}
	}

	struct TestEpochWitnesserGenerator {
		processed_blocks_sender: tokio::sync::mpsc::UnboundedSender<u64>,
		block_subscriber: BlockSubscriber,
	}

	impl TestEpochWitnesserGenerator {
		pub fn new() -> (async_channel::Sender<u64>, tokio::sync::mpsc::UnboundedReceiver<u64>, Self)
		{
			let (processed_blocks_sender, processed_blocks_receiver) =
				tokio::sync::mpsc::unbounded_channel();

			let (block_sender, block_receiver) = async_channel::unbounded();

			(
				block_sender,
				processed_blocks_receiver,
				TestEpochWitnesserGenerator {
					processed_blocks_sender,
					block_subscriber: BlockSubscriber { block_receiver },
				},
			)
		}
	}

	#[async_trait]
	impl EpochProcessGenerator for TestEpochWitnesserGenerator {
		type Witnesser = TestEpochWitnesser;

		async fn init(
			&mut self,
			epoch_start: EpochStart<cf_chains::Ethereum>,
		) -> anyhow::Result<WitnesserInitResult<TestEpochWitnesser>> {
			Ok(WitnesserInitResult::Created((
				TestEpochWitnesser {
					last_processed_block: epoch_start.block_number,
					processed_blocks_sender: self.processed_blocks_sender.clone(),
				},
				self.block_subscriber.block_stream_from(epoch_start.block_number),
			)))
		}
	}

	struct BlockSubscriber {
		block_receiver: async_channel::Receiver<u64>,
	}

	impl BlockSubscriber {
		fn block_stream_from(
			&mut self,
			block_number: u64,
		) -> Pin<Box<dyn Stream<Item = anyhow::Result<u64>> + Send>> {
			let block_receiver = self.block_receiver.clone();

			block_receiver
				.skip_while(move |block| futures::future::ready(*block < block_number))
				.map(Ok)
				.boxed()
		}
	}

	struct EpochStarter {
		epoch_index: u32,
		epoch_start_sender: async_broadcast::Sender<EpochStart<cf_chains::Ethereum>>,
	}

	impl EpochStarter {
		async fn start(&mut self, block_number: u64, participant: bool) {
			self.epoch_start_sender
				.broadcast(EpochStart {
					epoch_index: self.epoch_index,
					block_number,
					current: true,
					participant,
					data: (),
				})
				.await
				.unwrap();

			self.epoch_index += 1;
		}
	}

	#[tokio::test]
	async fn epoch_witnesser_only_processes_active_epochs() {
		use std::time::Duration;

		let (epoch_start_sender, epoch_start_receiver) = async_broadcast::broadcast(1);

		let (block_sender, mut processed_blocks_receiver, epoch_witnesser_generator) =
			TestEpochWitnesserGenerator::new();
		let mut epoch_starter = EpochStarter { epoch_index: 0, epoch_start_sender };

		let join_handle = tokio::spawn(start_epoch_process_runner(
			None,
			Arc::new(Mutex::new(epoch_start_receiver)),
			epoch_witnesser_generator,
			(),
		));

		use utilities::testing::expect_recv_with_timeout;

		// Send start of epoch from block 0
		epoch_starter.start(0, true).await;

		// Send block 0, should be witnessed
		block_sender.send(0).await.unwrap();
		assert_eq!(expect_recv_with_timeout(&mut processed_blocks_receiver).await, 0);

		// Send start of epoch from block 2 (not a participant), should still process block 1
		epoch_starter.start(2, false).await;
		// Add a small delay to prevent the witnesser from spuriously processing the next block
		// (if the events are received out of order):
		tokio::time::sleep(Duration::from_millis(10)).await;
		block_sender.send(1).await.unwrap();
		assert_eq!(expect_recv_with_timeout(&mut processed_blocks_receiver).await, 1);

		// Not active in epoch, so should ignore block 2
		block_sender.send(2).await.unwrap();
		assert_eq!(recv_with_timeout(&mut processed_blocks_receiver).await, None);

		// Send start of epoch from block 4 (participant), we should still ignore block 3
		epoch_starter.start(4, true).await;
		block_sender.send(3).await.unwrap();

		// Should process block 4, the first block in the new epoch
		block_sender.send(4).await.unwrap();
		assert_eq!(expect_recv_with_timeout(&mut processed_blocks_receiver).await, 4);

		// Send start of epoch from block 5 (non-participant), should ignore block 5
		epoch_starter.start(5, false).await;
		// Add a small delay to prevent the witnesser from spuriously processing the next block
		// (if the events are received out of order):
		tokio::time::sleep(Duration::from_millis(10)).await;
		block_sender.send(5).await.unwrap();
		assert_eq!(recv_with_timeout(&mut processed_blocks_receiver).await, None);

		// Start another epoch.
		epoch_starter.start(6, true).await;

		// Make the witnesser fail
		block_sender.send(u64::MAX).await.unwrap();

		let result = join_handle.await.unwrap();
		assert!(
			matches!(
				&result,
				Err(EpochProcessRunnerError::WitnesserError(epoch_start)) if epoch_start.epoch_index == epoch_starter.epoch_index - 1
			),
			"Expected error, got {:?}",
			&result
		);
	}
}
