use std::{
	collections::{BTreeMap, BTreeSet},
	sync::Arc,
};

use crate::{
	common::Signal,
	state_chain_observer::client::{
		storage_api::StorageApi,
		stream_api::{StreamApi, FINALIZED},
		STATE_CHAIN_CONNECTION,
	},
};
use cf_chains::Chain;
use cf_primitives::{AccountId, EpochIndex};
use futures::StreamExt;
use futures_core::{Future, Stream};
use futures_util::stream;
use state_chain_runtime::PalletInstanceAlias;
use utilities::{spmc, task_scope::Scope};

use super::{ActiveAndFuture, ExternalChain, RuntimeHasChain};

/// https://linear.app/chainflip/issue/PRO-877/external-chain-sources-can-block-sc-observer
const CHANNEL_BUFFER: usize = 128;

#[derive(Clone)]
pub struct Epoch<Info, HistoricInfo> {
	pub index: EpochIndex,
	pub info: Info,
	pub historic_signal: Signal<HistoricInfo>,
	pub expired_signal: Signal<()>,
}

#[derive(Clone)]
enum EpochUpdate<Info, HistoricInfo> {
	NewCurrent(Info),
	Historic(HistoricInfo),
	Expired,
}

#[derive(Clone)]
pub struct EpochSource<Info, HistoricInfo> {
	epochs: BTreeMap<EpochIndex, (Info, Option<HistoricInfo>)>,
	epoch_update_receiver:
		spmc::Receiver<(EpochIndex, state_chain_runtime::Hash, EpochUpdate<Info, HistoricInfo>)>,
}

impl<'a, 'env, StateChainClient, Info, HistoricInfo>
	From<EpochSourceBuilder<'a, 'env, StateChainClient, Info, HistoricInfo>>
	for EpochSource<Info, HistoricInfo>
{
	fn from(builder: EpochSourceBuilder<'a, 'env, StateChainClient, Info, HistoricInfo>) -> Self {
		Self { epochs: builder.epochs, epoch_update_receiver: builder.epoch_update_receiver }
	}
}

pub struct EpochSourceBuilder<'a, 'env, StateChainClient, Info, HistoricInfo> {
	scope: &'a Scope<'env, anyhow::Error>,
	state_chain_client: Arc<StateChainClient>,
	initial_block_hash: state_chain_runtime::Hash,
	epochs: BTreeMap<EpochIndex, (Info, Option<HistoricInfo>)>,
	epoch_update_receiver:
		spmc::Receiver<(EpochIndex, state_chain_runtime::Hash, EpochUpdate<Info, HistoricInfo>)>,
}
impl<'a, 'env, StateChainClient, Info: Clone, HistoricInfo: Clone> Clone
	for EpochSourceBuilder<'a, 'env, StateChainClient, Info, HistoricInfo>
{
	fn clone(&self) -> Self {
		Self {
			scope: self.scope,
			state_chain_client: self.state_chain_client.clone(),
			initial_block_hash: self.initial_block_hash,
			epochs: self.epochs.clone(),
			epoch_update_receiver: self.epoch_update_receiver.clone(),
		}
	}
}
impl EpochSource<(), ()> {
	pub async fn builder<
		'a,
		'env,
		StateChainStream: StreamApi<FINALIZED>,
		StateChainClient: StorageApi + Send + Sync + 'static,
	>(
		scope: &'a Scope<'env, anyhow::Error>,
		mut state_chain_stream: StateChainStream,
		state_chain_client: Arc<StateChainClient>,
	) -> EpochSourceBuilder<'a, 'env, StateChainClient, (), ()> {
		let (epoch_update_sender, epoch_update_receiver) = spmc::channel(CHANNEL_BUFFER);

		let initial_block_hash = state_chain_stream.cache().hash;

		let mut current_epoch = state_chain_client
			.storage_value::<pallet_cf_validator::CurrentEpoch<state_chain_runtime::Runtime>>(
				initial_block_hash,
			)
			.await
			.expect(STATE_CHAIN_CONNECTION);
		let mut historic_epochs = BTreeSet::from_iter(
			state_chain_client
				.storage_map::<pallet_cf_validator::EpochExpiries<state_chain_runtime::Runtime>, Vec<_>>(
					initial_block_hash,
				)
				.await
				.expect(STATE_CHAIN_CONNECTION)
				.into_iter()
				.map(|(_, index)| index),
		);
		assert!(!historic_epochs.contains(&current_epoch));

		let active_epochs = historic_epochs
			.iter()
			.map(|epoch| (*epoch, ((), Some(()))))
			.chain(std::iter::once((current_epoch, ((), None))))
			.collect();

		scope.spawn({
			let state_chain_client = state_chain_client.clone();
			async move {
				utilities::loop_select! {
					let _ = epoch_update_sender.closed() => { break Ok(()) },
					if let Some(block) = state_chain_stream.next() => {
						let old_current_epoch = std::mem::replace(&mut current_epoch, state_chain_client
							.storage_value::<pallet_cf_validator::CurrentEpoch<
								state_chain_runtime::Runtime,
							>>(block.hash)
							.await
							.expect(STATE_CHAIN_CONNECTION));
						if old_current_epoch != current_epoch {
							epoch_update_sender.send((old_current_epoch, block.hash, EpochUpdate::Historic(()))).await;
							epoch_update_sender.send((current_epoch, block.hash, EpochUpdate::NewCurrent(()))).await;
							historic_epochs.insert(old_current_epoch);
						}

						let old_historic_epochs = std::mem::replace(&mut historic_epochs, BTreeSet::from_iter(
							state_chain_client.storage_map::<pallet_cf_validator::EpochExpiries<
								state_chain_runtime::Runtime,
							>, Vec<_>>(block.hash).await.expect(STATE_CHAIN_CONNECTION).into_iter().map(|(_, index)| index)
						));
						assert!(!historic_epochs.contains(&current_epoch));
						assert!(old_historic_epochs.is_superset(&historic_epochs));
						for expired_epoch in old_historic_epochs.difference(&historic_epochs) {
							epoch_update_sender.send((*expired_epoch, block.hash, EpochUpdate::Expired)).await;
						}
					} else break Ok(()),
				}
			}
		});

		EpochSourceBuilder {
			scope,
			state_chain_client,
			initial_block_hash,
			epochs: active_epochs,
			epoch_update_receiver,
		}
	}
}

impl<Info: Clone + Send + Sync + 'static, HistoricInfo: Clone + Send + Sync + 'static>
	EpochSource<Info, HistoricInfo>
{
	pub async fn into_stream(
		self,
	) -> ActiveAndFuture<
		impl Iterator<Item = Epoch<Info, HistoricInfo>> + Send + 'static,
		impl Stream<Item = Epoch<Info, HistoricInfo>> + Send + 'static,
	> {
		let mut historic_signallers = BTreeMap::new();
		let mut expired_signallers = BTreeMap::new();

		ActiveAndFuture {
			active: self
				.epochs
				.into_iter()
				.map(|(index, (info, option_historic_info))| {
					let (expired_signaller, expired_signal) = Signal::new();

					expired_signallers.insert(index, expired_signaller);

					Epoch {
						index,
						info,
						historic_signal: match option_historic_info {
							Some(historic_info) => Signal::signalled(historic_info),
							None => {
								let (historic_signaller, historic_signal) = Signal::new();
								historic_signallers.insert(index, historic_signaller);
								historic_signal
							},
						},
						expired_signal,
					}
				})
				.collect::<Vec<_>>()
				.into_iter(),
			future: stream::unfold(
				(self.epoch_update_receiver, historic_signallers, expired_signallers),
				|(mut epoch_update_receiver, mut historic_signallers, mut expired_signallers)| async move {
					while let Some((index, _block_hash, update)) =
						epoch_update_receiver.next().await
					{
						match update {
							EpochUpdate::NewCurrent(info) => {
								let (historic_signaller, historic_signal) = Signal::new();
								let (expired_signaller, expired_signal) = Signal::new();

								historic_signallers.insert(index, historic_signaller);
								expired_signallers.insert(index, expired_signaller);

								return Some((
									Epoch { index, info, historic_signal, expired_signal },
									(
										epoch_update_receiver,
										historic_signallers,
										expired_signallers,
									),
								))
							},
							EpochUpdate::Historic(historic_info) => {
								historic_signallers.remove(&index).unwrap().signal(historic_info);
							},
							EpochUpdate::Expired => {
								expired_signallers.remove(&index).unwrap().signal(());
							},
						}
					}

					None
				},
			),
		}
	}
}

impl<
		'a,
		'env,
		StateChainClient: StorageApi + Send + Sync + 'static,
		Info: Clone + Send + Sync + 'static,
		HistoricInfo: Clone + Send + Sync + 'static,
	> EpochSourceBuilder<'a, 'env, StateChainClient, Info, HistoricInfo>
{
	/// Only keep the epochs where the given account is participating in that epoch as an authority.
	pub async fn participating(
		self,
		account_id: AccountId,
	) -> EpochSourceBuilder<'a, 'env, StateChainClient, Info, HistoricInfo> {
		self.filter_map(
			move |state_chain_client, epoch, block_hash, info| {
				let account_id = account_id.clone();
				async move {
					if state_chain_client
						.storage_map_entry::<pallet_cf_validator::HistoricalActiveEpochs<state_chain_runtime::Runtime>>(
							block_hash,
							&account_id,
						)
						.await
						.expect(STATE_CHAIN_CONNECTION)
						.iter()
						.any(|participating_epoch| *participating_epoch == epoch)
					{
						Some(info)
					} else {
						None
					}
				}
			},
			|_state_chain_client, _epoch, _block_hash, historic_info| async move { historic_info },
		)
		.await
	}

	/// Filter out the epochs where the provided `filter_map` returns `None`, mapping the epoch
	/// info. Just map the historic info, without filtering anything based on it.
	pub async fn filter_map<
		FilterMapInfo,
		InfoFut,
		MappedInfo,
		MapHistoricInfo,
		HIFut,
		MappedHistoricInfo,
	>(
		self,
		filter_map: FilterMapInfo,
		map_historic_info: MapHistoricInfo,
	) -> EpochSourceBuilder<'a, 'env, StateChainClient, MappedInfo, MappedHistoricInfo>
	where
		FilterMapInfo: Fn(Arc<StateChainClient>, EpochIndex, state_chain_runtime::Hash, Info) -> InfoFut
			+ Send
			+ Sync
			+ 'static,
		InfoFut: Future<Output = Option<MappedInfo>> + Send + 'static,
		MappedInfo: Clone + Send + Sync + 'static,
		MapHistoricInfo: Fn(Arc<StateChainClient>, EpochIndex, state_chain_runtime::Hash, HistoricInfo) -> HIFut
			+ Send
			+ Sync
			+ 'static,
		HIFut: Future<Output = MappedHistoricInfo> + Send + 'static,
		MappedHistoricInfo: Clone + Send + Sync + 'static,
	{
		let EpochSourceBuilder {
			scope,
			state_chain_client,
			initial_block_hash,
			epochs: unmapped_epochs,
			epoch_update_receiver: mut unmapped_epoch_update_receiver,
		} = self;

		let (epoch_update_sender, epoch_update_receiver) = spmc::channel(CHANNEL_BUFFER);

		let epochs: BTreeMap<_, _> = futures::stream::iter(unmapped_epochs)
			.filter_map(|(epoch, (info, option_historic_info))| {
				let filter_map = &filter_map;
				let map_historic_info = &map_historic_info;
				let state_chain_client = &state_chain_client;
				async move {
					if let Some(mapped_info) =
						filter_map(state_chain_client.clone(), epoch, initial_block_hash, info)
							.await
					{
						Some((
							epoch,
							(
								mapped_info,
								match option_historic_info {
									Some(historic_info) => Some(
										map_historic_info(
											state_chain_client.clone(),
											epoch,
											initial_block_hash,
											historic_info,
										)
										.await,
									),
									None => None,
								},
							),
						))
					} else {
						None
					}
				}
			})
			.collect()
			.await;

		self.scope.spawn({
			let state_chain_client = state_chain_client.clone();
			let mut epochs = epochs.keys().cloned().collect::<BTreeSet<_>>();
			async move {
				utilities::loop_select! {
					let _ = epoch_update_sender.closed() => { break Ok(()) },
					if let Some((epoch, block_hash, update)) = unmapped_epoch_update_receiver.next() => {
						match update {
							EpochUpdate::NewCurrent(info) => {
								if let Some(mapped_info) = filter_map(state_chain_client.clone(), epoch, block_hash, info).await {
									epochs.insert(epoch);
									epoch_update_sender.send((epoch, block_hash, EpochUpdate::NewCurrent(mapped_info))).await;
								}
							},
							EpochUpdate::Historic(historic_info) => {
								if epochs.contains(&epoch) {
									epoch_update_sender.send((
										epoch,
										block_hash,
										EpochUpdate::Historic(map_historic_info(state_chain_client.clone(), epoch, block_hash, historic_info).await),
									)).await;
								}
							},
							EpochUpdate::Expired => {
								epochs.remove(&epoch);
							},
						}
					} else break Ok(()),
				}
			}
		});

		EpochSourceBuilder {
			scope,
			state_chain_client,
			initial_block_hash,
			epochs,
			epoch_update_receiver,
		}
	}
}

pub type Vault<TChain, ExtraInfo, ExtraHistoricInfo> = Epoch<
	(pallet_cf_vaults::Vault<TChain>, ExtraInfo),
	(<TChain as Chain>::ChainBlockNumber, ExtraHistoricInfo),
>;

pub type VaultSource<TChain, ExtraInfo, ExtraHistoricInfo> = EpochSource<
	(pallet_cf_vaults::Vault<TChain>, ExtraInfo),
	(<TChain as Chain>::ChainBlockNumber, ExtraHistoricInfo),
>;

impl<'a, 'env, StateChainClient: StorageApi + Send + Sync + 'static, Info, HistoricInfo>
	EpochSourceBuilder<'a, 'env, StateChainClient, Info, HistoricInfo>
{
	/// Get all the vaults for each each epoch for a particular chain.
	/// Not all epochs will have all vaults. For example, the first epoch will not have a vault for
	/// Polkadot or Bitcoin.
	pub async fn vaults<TChain: ExternalChain>(
		self,
	) -> EpochSourceBuilder<
		'a,
		'env,
		StateChainClient,
		(pallet_cf_vaults::Vault<TChain>, Info),
		(<TChain as Chain>::ChainBlockNumber, HistoricInfo),
	>
	where
		state_chain_runtime::Runtime: RuntimeHasChain<TChain>,
		Info: Clone + Send + Sync + 'static,
		HistoricInfo: Clone + Send + Sync + 'static,
	{
		self.filter_map(
			|state_chain_client, epoch, block_hash, info| async move {
				state_chain_client
					.storage_map_entry::<pallet_cf_vaults::Vaults<
						state_chain_runtime::Runtime,
						<TChain as PalletInstanceAlias>::Instance,
					>>(block_hash, &epoch)
					.await
					.expect(STATE_CHAIN_CONNECTION)
					.map(|vault| (vault, info))
			},
			|state_chain_client, epoch, block_hash, historic_info| async move {
				(
					state_chain_client
						.storage_map_entry::<pallet_cf_vaults::Vaults<
							state_chain_runtime::Runtime,
							<TChain as PalletInstanceAlias>::Instance,
						>>(block_hash, &(epoch + 1))
						.await
						.expect(STATE_CHAIN_CONNECTION)
						.expect("We know the epoch ended, so the next vault must exist.")
						.active_from_block,
					historic_info,
				)
			},
		)
		.await
	}
}
