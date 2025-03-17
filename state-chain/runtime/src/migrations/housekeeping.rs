use crate::{chainflip::address_derivation::AddressDerivation, Runtime};
use cf_chains::{
	address::AddressDerivationApi,
	assets::{self},
	btc::{self, Utxo, UtxoId},
	instances::{ArbitrumInstance, BitcoinInstance, EthereumInstance, PolkadotInstance},
	Bitcoin, DepositChannel,
};
use cf_runtime_upgrade_utilities::genesis_hashes;
#[cfg(feature = "try-runtime")]
use cf_traits::BalanceApi;
use cf_traits::{DepositApi, IngressSink};
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use pallet_cf_broadcast::migrations::remove_aborted_broadcasts;
use sp_runtime::AccountId32;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub struct Migration;

const AMOUNT: u64 = 4_0418_3067;
const CHANNEL_ID: u64 = 44260;
const BTC: assets::btc::Asset = assets::btc::Asset::Btc;
const ACCOUNT: AccountId32 = AccountId32::new(hex_literal::hex!(
	"a01c278b9262bdea45f3c33efe71b06ad3a747263273c796efd9192b70851626"
));

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				log::info!("🧹 Housekeeping, removing stale aborted broadcasts");
				remove_aborted_broadcasts::remove_stale_and_all_older::<Runtime, EthereumInstance>(
					remove_aborted_broadcasts::ETHEREUM_MAX_ABORTED_BROADCAST_BERGHAIN,
				);
				remove_aborted_broadcasts::remove_stale_and_all_older::<Runtime, ArbitrumInstance>(
					remove_aborted_broadcasts::ARBITRUM_MAX_ABORTED_BROADCAST_BERGHAIN,
				);
				log::info!("🧹 Housekeeping, recover missing utxo.");
				if crate::VERSION.spec_version == 1_07_12 &&
					!pallet_cf_ingress_egress::DepositChannelPool::<
						Runtime,
						BitcoinInstance,
					>::contains_key(CHANNEL_ID) {
					let (script_pubkey, deposit_address) = <AddressDerivation as AddressDerivationApi<Bitcoin>>::generate_address_and_state(
							BTC,
							CHANNEL_ID,
						).expect("can only fail if channel is out of bounds or there is no vault key.");
					pallet_cf_ingress_egress::DepositChannelPool::<
						Runtime,
						BitcoinInstance,
					>::insert(CHANNEL_ID, DepositChannel::<Bitcoin> {
						channel_id: CHANNEL_ID,
						address: script_pubkey.clone(),
						asset: BTC,
						state: deposit_address.clone(),
					});
					if pallet_cf_ingress_egress::Pallet::<Runtime, BitcoinInstance>::request_liquidity_deposit_address(
						ACCOUNT, BTC,
						30,
						cf_chains::ForeignChainAddress::Btc(btc::ScriptPubkey::P2SH(hex_literal::hex!("73eb72d4cb86650ddfb50b4236553bd6ebd70253"))),
					).is_ok() {
						log::info!("₿ Channel opened, triggering ingress.");
						pallet_cf_ingress_egress::Pallet::<Runtime, BitcoinInstance>::on_ingress(
							script_pubkey, BTC, AMOUNT, 887906, Utxo {
							id: UtxoId {
								tx_id: sp_core::H256(hex_literal::hex!("33e68b045e81b8513e4c485fccc87babde2bcc16a2332eaff433b154f47a89ed")),
								vout: 68,
							},
							amount: AMOUNT,
							deposit_address,
						});
					} else {
						log::error!("🚨 Failed to open channel.");
					};
				} else {
					log::info!("⏭ Utxo already recovered.");
				}
			},
			genesis_hashes::PERSEVERANCE => {
				log::info!("🧹 Housekeeping, removing stale aborted broadcasts");
				remove_aborted_broadcasts::remove_stale_and_all_older::<Runtime, EthereumInstance>(
					remove_aborted_broadcasts::ETHEREUM_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
				remove_aborted_broadcasts::remove_stale_and_all_older::<Runtime, ArbitrumInstance>(
					remove_aborted_broadcasts::ARBITRUM_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
				remove_aborted_broadcasts::remove_stale_and_all_older::<Runtime, PolkadotInstance>(
					remove_aborted_broadcasts::POLKADOT_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
			},
			genesis_hashes::SISYPHOS => {
				log::info!("🧹 No housekeeping required for Sisyphos.");
			},
			_ => {},
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				let pre_upgrade_balance =
					pallet_cf_asset_balances::Pallet::<Runtime>::get_balance(&ACCOUNT, BTC.into());
				Ok(pre_upgrade_balance.to_le_bytes().to_vec())
			},
			_ => Ok(Default::default()),
		}
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				log::info!(
					"Housekeeping post_upgrade, checking stale aborted broadcasts are removed."
				);
				remove_aborted_broadcasts::assert_removed::<Runtime, EthereumInstance>(
					remove_aborted_broadcasts::ETHEREUM_MAX_ABORTED_BROADCAST_BERGHAIN,
				);
				remove_aborted_broadcasts::assert_removed::<Runtime, ArbitrumInstance>(
					remove_aborted_broadcasts::ARBITRUM_MAX_ABORTED_BROADCAST_BERGHAIN,
				);
				let pre_upgrade_balance = u128::from_le_bytes(state.as_slice().try_into().unwrap());
				let post_upgrade_balance =
					pallet_cf_asset_balances::Pallet::<Runtime>::get_balance(&ACCOUNT, BTC.into());
				log::info!(
					"Btc balance pre-upgrade: {}, post-upgrade: {}, increase: {}",
					pre_upgrade_balance,
					post_upgrade_balance,
					post_upgrade_balance - pre_upgrade_balance
				);
				assert!(pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::get().iter().any(
					|utxo| {
						utxo.id ==
							UtxoId {
								tx_id: sp_core::H256(hex_literal::hex!(
								"33e68b045e81b8513e4c485fccc87babde2bcc16a2332eaff433b154f47a89ed"
							)),
								vout: 68,
							}
					}
				));
			},
			genesis_hashes::PERSEVERANCE => {
				log::info!(
					"Housekeeping post_upgrade, checking stale aborted broadcasts are removed."
				);
				remove_aborted_broadcasts::assert_removed::<Runtime, EthereumInstance>(
					remove_aborted_broadcasts::ETHEREUM_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
				remove_aborted_broadcasts::assert_removed::<Runtime, ArbitrumInstance>(
					remove_aborted_broadcasts::ARBITRUM_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
				remove_aborted_broadcasts::assert_removed::<Runtime, PolkadotInstance>(
					remove_aborted_broadcasts::POLKADOT_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
			},
			genesis_hashes::SISYPHOS => {
				log::info!("Skipping housekeeping post_upgrade for Sisyphos.");
			},
			_ => {},
		}
		Ok(())
	}
}
