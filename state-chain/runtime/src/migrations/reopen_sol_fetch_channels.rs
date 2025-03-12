use core::str::FromStr;

use cf_primitives::ChannelId;
use frame_support::traits::OnRuntimeUpgrade;
use pallet_cf_ingress_egress::{BoostStatus, DepositChannelDetails, DepositChannelLookup};
use sp_core::crypto::Ss58Codec;

use crate::*;
use frame_support::pallet_prelude::Weight;
use sp_runtime::DispatchError;

use cf_chains::DepositChannel;
use codec::{Decode, Encode};

pub struct Migration;

const NUMBER_OF_REOPENED_CHANNELS: u32 = 6;

// Tests for this migration are in:
// state-chain/cf-integration-tests/src/migrations/serialize_solana_broadcast.rs
impl OnRuntimeUpgrade for Migration {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let open_channels_before =
			DepositChannelLookup::<Runtime, SolanaInstance>::iter().count() as u32;
		Ok(open_channels_before.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		use pallet_cf_ingress_egress::{FetchOrTransfer, ScheduledEgressFetchOrTransfer};

		let current_chain_block_number = SolanaChainTrackingProvider::get_block_height();

		let reopened_channels: [(&str, ChannelId); NUMBER_OF_REOPENED_CHANNELS as usize] = [
			("5yWUEA6tKGTdUb3zR25Cox9dqKVGCGBJwrvkFQsUQ9FQ", 20575),
			("gWRXPE6zeF8pBVi5KP4h5LT9VSb2rS6ZMmnH8dgdCgu", 20581),
			("DgypQfZmKgAHzAJdW9fQ3P7jRQrxA9HFxhTR2ZQegALr", 20819),
			("E1tiLwL5dBbPS3SQuYYLfiqH11gzVPc3WYiXYcRmqrBN", 26394),
			("ExZmpJJLq21hNJCqy6Bz9hftLaV58fB2ppzYPk2U9g4U", 40616),
			("5zUYmysTSiitS17dePrMwYvzhU4tNSaFfLa87td5C9fw", 41470),
		];

		// From mainnet chain.
		let address_to_channel_id: BTreeMap<SolAddress, ChannelId> = reopened_channels
			.into_iter()
			.map(|(address, channel_id)| (SolAddress::from_str(address).unwrap(), channel_id))
			.collect::<BTreeMap<_, _>>();

		for scheduled_fetch in ScheduledEgressFetchOrTransfer::<Runtime, SolanaInstance>::get() {
			match scheduled_fetch {
				FetchOrTransfer::Fetch { asset, deposit_address, .. } => {
					// Note: These channels will not be closed at the expiry period, since they are
					// normally closed by the election, which there is not one of.
					// Thus they should be closed later by a migration.

					// Just need any account id, not important which.
					let cf_broker = sp_runtime::AccountId32::from_ss58check(
						"cFLRQDfEdmnv6d2XfHJNRBQHi4fruPMReLSfvB8WWD2ENbqj7",
					)
					.unwrap();

					// Don't override any already open channels.
					if !DepositChannelLookup::<Runtime, SolanaInstance>::contains_key(
						deposit_address,
					) {
						if let Some(channel_id) = address_to_channel_id.get(&deposit_address) {
							DepositChannelLookup::<Runtime, SolanaInstance>::insert(
								deposit_address,
								DepositChannelDetails {
									owner: cf_broker.clone(),
									deposit_channel: DepositChannel {
										channel_id: *channel_id,
										address: deposit_address,
										asset,
										// AccountBump (u8) - what is this used for
										state: 0u8,
									},
									opened_at: current_chain_block_number,
									// 172800 is the number of blocks in 1 days, at a block/500ms.
									expires_at: current_chain_block_number + 172800,
									action: ChannelAction::LiquidityProvision {
										lp_account: cf_broker,
										refund_address: None,
									},
									boost_fee: 0,
									boost_status: BoostStatus::NotBoosted,
								},
							);
						}
					}
				},
				_ => {},
			}
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let number_of_open_channels_after =
			DepositChannelLookup::<Runtime, SolanaInstance>::iter().count() as u32;
		let number_of_open_channels_before: u32 = Decode::decode(&mut &state[..]).unwrap();

		if number_of_open_channels_after !=
			number_of_open_channels_before + NUMBER_OF_REOPENED_CHANNELS
		{
			return Err(DispatchError::Other("Number of open channels did not increase by 6."));
		}
		Ok(())
	}
}

pub struct NoopUpgrade;

impl OnRuntimeUpgrade for NoopUpgrade {
	fn on_runtime_upgrade() -> Weight {
		Weight::zero()
	}
}
