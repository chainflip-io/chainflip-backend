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

use super::decl_api::{self, *};
use crate::runtime_apis::types::*;
use sp_api::impl_runtime_apis;

use crate::{chainflip::Offence, Runtime};

use cf_primitives::{AssetAmount, ForeignChain};
use core::str;
pub use pallet_cf_ingress_egress::ChannelAction;
pub use pallet_cf_lending_pools::BoostPoolDetails;
use scale_info::prelude::string::String;
use sp_std::vec::Vec;

impl_runtime_apis! {
	impl decl_api::MonitoringRuntimeApi<Block> for Runtime {
		fn cf_authorities() -> AuthoritiesInfo {
			let mut authorities = pallet_cf_validator::CurrentAuthorities::<Runtime>::get();
			let mut result = AuthoritiesInfo {
				authorities: authorities.len() as u32,
				online_authorities: 0,
				backups: 0,
				online_backups: 0,
			};
			authorities.retain(HeartbeatQualification::<Runtime>::is_qualified);
			result.online_authorities = authorities.len() as u32;
			result
		}

		fn cf_external_chains_block_height() -> ExternalChainsBlockHeight {
			// safe to unwrap these value as stated on the storage item doc
			let btc = pallet_cf_chain_tracking::CurrentChainState::<Runtime, BitcoinInstance>::get().unwrap();
			let eth = pallet_cf_chain_tracking::CurrentChainState::<Runtime, EthereumInstance>::get().unwrap();
			let dot = pallet_cf_chain_tracking::CurrentChainState::<Runtime, PolkadotInstance>::get().unwrap();
			let arb = pallet_cf_chain_tracking::CurrentChainState::<Runtime, ArbitrumInstance>::get().unwrap();
			let sol = SolanaChainTrackingProvider::get_block_height();
			let hub = pallet_cf_chain_tracking::CurrentChainState::<Runtime, AssethubInstance>::get().unwrap();

			ExternalChainsBlockHeight {
				bitcoin: btc.block_height,
				ethereum: eth.block_height,
				polkadot: dot.block_height.into(),
				solana: sol,
				arbitrum: arb.block_height,
				assethub: hub.block_height.into(),
			}
		}

		fn cf_btc_utxos() -> BtcUtxos {
			let utxos = pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::get();
			let mut btc_balance = utxos.iter().fold(0, |acc, elem| acc + elem.amount);
			//Sum the btc balance contained in the change utxos to the btc "free_balance"
			let btc_ceremonies = pallet_cf_threshold_signature::PendingCeremonies::<Runtime,BitcoinInstance>::iter_values().map(|ceremony|{
				ceremony.request_context.request_id
			}).collect::<Vec<_>>();
			let EpochKey { key, .. } = pallet_cf_threshold_signature::Pallet::<Runtime, BitcoinInstance>::active_epoch_key()
				.expect("We should always have a key for the current epoch");
			for ceremony in btc_ceremonies {
				if let RuntimeCall::BitcoinBroadcaster(pallet_cf_broadcast::pallet::Call::on_signature_ready{ api_call, ..}) = pallet_cf_threshold_signature::RequestCallback::<Runtime, BitcoinInstance>::get(ceremony).unwrap() {
					if let BitcoinApi::BatchTransfer(batch_transfer) = *api_call {
						for output in batch_transfer.bitcoin_transaction.outputs {
							if [
								ScriptPubkey::Taproot(key.previous.unwrap_or_default()),
								ScriptPubkey::Taproot(key.current),
							]
							.contains(&output.script_pubkey)
							{
								btc_balance += output.amount;
							}
						}
					}
				}
			}
			BtcUtxos {
				total_balance: btc_balance,
				count: utxos.len() as u32,
			}
		}

		fn cf_dot_aggkey() -> PolkadotAccountId {
			let epoch = PolkadotThresholdSigner::current_key_epoch().unwrap_or_default();
			PolkadotThresholdSigner::keys(epoch).unwrap_or_default()
		}

		fn cf_suspended_validators() -> Vec<(Offence, u32)> {
			let suspended_for_keygen = match pallet_cf_validator::Pallet::<Runtime>::current_rotation_phase() {
				pallet_cf_validator::RotationPhase::KeygensInProgress(rotation_state) |
				pallet_cf_validator::RotationPhase::KeyHandoversInProgress(rotation_state) |
				pallet_cf_validator::RotationPhase::ActivatingKeys(rotation_state) |
				pallet_cf_validator::RotationPhase::NewKeysActivated(rotation_state) => { rotation_state.banned.len() as u32 },
				_ => {0u32}
			};
			pallet_cf_reputation::Suspensions::<Runtime>::iter().map(|(key, _)| {
				if key == pallet_cf_threshold_signature::PalletOffence::FailedKeygen.into() {
					return (key, suspended_for_keygen);
				}
				(key, pallet_cf_reputation::Pallet::<Runtime>::validators_suspended_for(&[key]).len() as u32)
			}).collect()
		}
		fn cf_epoch_state() -> EpochState {
			let auction_params = Validator::auction_parameters();
			let min_active_bid = SetSizeMaximisingAuctionResolver::try_new(
				<Runtime as Chainflip>::EpochInfo::current_authority_count(),
				auction_params,
			)
			.and_then(|resolver| {
				resolver.resolve_auction(
					Validator::get_qualified_bidders::<<Runtime as pallet_cf_validator::Config>::KeygenQualification>(),
				)
			})
			.ok()
			.map(|auction_outcome| auction_outcome.bond);
			EpochState {
				epoch_duration: Validator::epoch_duration(),
				current_epoch_started_at: Validator::current_epoch_started_at(),
				current_epoch_index: Validator::current_epoch(),
				min_active_bid,
				rotation_phase: Validator::current_rotation_phase().to_str().to_string(),
			}
		}
		fn cf_redemptions() -> RedemptionsInfo {
			let redemptions: Vec<_> = pallet_cf_funding::PendingRedemptions::<Runtime>::iter().collect();
			RedemptionsInfo {
				total_balance: redemptions.iter().fold(0, |acc, elem| acc + elem.1.total),
				count: redemptions.len() as u32,
			}
		}
		fn cf_pending_broadcasts_count() -> PendingBroadcasts {
			PendingBroadcasts {
				ethereum: pallet_cf_broadcast::PendingBroadcasts::<Runtime, EthereumInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				bitcoin: pallet_cf_broadcast::PendingBroadcasts::<Runtime, BitcoinInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				polkadot: pallet_cf_broadcast::PendingBroadcasts::<Runtime, PolkadotInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				arbitrum: pallet_cf_broadcast::PendingBroadcasts::<Runtime, ArbitrumInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				solana: pallet_cf_broadcast::PendingBroadcasts::<Runtime, SolanaInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				assethub: pallet_cf_broadcast::PendingBroadcasts::<Runtime, AssethubInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
			}
		}
		fn cf_pending_tss_ceremonies_count() -> PendingTssCeremonies {
			PendingTssCeremonies {
				evm: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, EvmInstance>::iter().collect::<Vec<_>>().len() as u32,
				bitcoin: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, BitcoinInstance>::iter().collect::<Vec<_>>().len() as u32,
				polkadot: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, PolkadotCryptoInstance>::iter().collect::<Vec<_>>().len() as u32,
				solana: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, SolanaInstance>::iter().collect::<Vec<_>>().len() as u32,
			}
		}
		fn cf_pending_swaps_count() -> u32 {
			pallet_cf_swapping::ScheduledSwaps::<Runtime>::get().len() as u32
		}
		fn cf_open_deposit_channels_count() -> OpenDepositChannels {
			fn open_channels<BlockHeight, I: 'static>() -> u32
				where BlockHeight: GetBlockHeight<<Runtime as pallet_cf_ingress_egress::Config<I>>::TargetChain>, Runtime: pallet_cf_ingress_egress::Config<I>
			{
				pallet_cf_ingress_egress::DepositChannelLookup::<Runtime, I>::iter().filter(|(_key, elem)| elem.expires_at > BlockHeight::get_block_height()).collect::<Vec<_>>().len() as u32
			}

			OpenDepositChannels{
				ethereum: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, EthereumInstance>, EthereumInstance>(),
				bitcoin: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, BitcoinInstance>, BitcoinInstance>(),
				polkadot: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, PolkadotInstance>, PolkadotInstance>(),
				arbitrum: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, ArbitrumInstance>, ArbitrumInstance>(),
				solana: open_channels::<SolanaChainTrackingProvider, SolanaInstance>(),
				assethub: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, AssethubInstance>, AssethubInstance>(),
			}
		}
		fn cf_fee_imbalance() -> FeeImbalance<AssetAmount> {
			FeeImbalance {
				ethereum: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Ethereum.gas_asset()),
				polkadot: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Polkadot.gas_asset()),
				arbitrum: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Arbitrum.gas_asset()),
				bitcoin: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Bitcoin.gas_asset()),
				solana: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Solana.gas_asset()),
				assethub: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Assethub.gas_asset()),
			}
		}
		fn cf_build_version() -> LastRuntimeUpgradeInfo {
			let info = frame_system::LastRuntimeUpgrade::<Runtime>::get().expect("this has to be set");
			LastRuntimeUpgradeInfo {
				spec_version: info.spec_version.into(),
				spec_name: info.spec_name,
			}
		}
		fn cf_rotation_broadcast_ids() -> ActivateKeysBroadcastIds{
			ActivateKeysBroadcastIds{
				ethereum: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, EthereumInstance>::get().map(|val| val.1),
				bitcoin: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, BitcoinInstance>::get().map(|val| val.1),
				polkadot: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, PolkadotInstance>::get().map(|val| val.1),
				arbitrum: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, ArbitrumInstance>::get().map(|val| val.1),
				solana: {
					let broadcast_id = pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, SolanaInstance>::get().map(|val| val.1);
					(broadcast_id, pallet_cf_broadcast::AwaitingBroadcast::<Runtime, SolanaInstance>::get(broadcast_id.unwrap_or_default()).map(|broadcast_data| broadcast_data.transaction_out_id))
				},
				assethub: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, AssethubInstance>::get().map(|val| val.1),
			}
		}
		fn cf_sol_nonces() -> SolanaNonces{
			SolanaNonces {
				available: pallet_cf_environment::SolanaAvailableNonceAccounts::<Runtime>::get(),
				unavailable: pallet_cf_environment::SolanaUnavailableNonceAccounts::<Runtime>::iter_keys().collect()
			}
		}
		fn cf_sol_aggkey() -> SolAddress{
			let epoch = SolanaThresholdSigner::current_key_epoch().unwrap_or_default();
			SolanaThresholdSigner::keys(epoch).unwrap_or_default()
		}
		fn cf_sol_onchain_key() -> SolAddress{
			SolanaBroadcaster::current_on_chain_key().unwrap_or_default()
		}
		fn cf_monitoring_data() -> MonitoringDataV2 {
			MonitoringDataV2 {
				external_chains_height: Self::cf_external_chains_block_height(),
				btc_utxos: Self::cf_btc_utxos(),
				epoch: Self::cf_epoch_state(),
				pending_redemptions: Self::cf_redemptions(),
				pending_broadcasts: Self::cf_pending_broadcasts_count(),
				pending_tss: Self::cf_pending_tss_ceremonies_count(),
				open_deposit_channels: Self::cf_open_deposit_channels_count(),
				fee_imbalance: Self::cf_fee_imbalance(),
				authorities: Self::cf_authorities(),
				build_version: Self::cf_build_version(),
				suspended_validators: Self::cf_suspended_validators(),
				pending_swaps: Self::cf_pending_swaps_count(),
				dot_aggkey: Self::cf_dot_aggkey(),
				flip_supply: {
					let flip = Self::cf_flip_supply();
					FlipSupply { total_supply: flip.0, offchain_supply: flip.1}
				},
				sol_aggkey: Self::cf_sol_aggkey(),
				sol_onchain_key: Self::cf_sol_onchain_key(),
				sol_nonces: Self::cf_sol_nonces(),
				activating_key_broadcast_ids: Self::cf_rotation_broadcast_ids(),
			}
		}
		fn cf_accounts_info(accounts: BoundedVec<AccountId, ConstU32<10>>) -> Vec<ValidatorInfo> {
			accounts.iter().map(|account_id| {
				Self::cf_validator_info(account_id)
			}).collect()
		}
	}
}
