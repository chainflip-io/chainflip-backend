use crate::*;
use cf_chains::{
	assets::eth::Asset as EthAsset, deposit_channel::DepositChannel, evm::DeploymentStatus,
};
use cf_primitives::Asset;
use cf_traits::BalanceApi;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use pallet_cf_ingress_egress::{
	BoostStatus, ChannelAction, DepositChannelDetails, DepositChannelLookup,
	DepositChannelRecycleBlocks, FetchOrTransfer, ProcessedUpTo, ScheduledEgressFetchOrTransfer,
};
use sp_core::{crypto::Ss58Codec, H160};
use sp_runtime::AccountId32;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
use sp_std::collections::btree_set::BTreeSet;

// Channels that need contract deployment + balance credit.
// (channel_id, address, amount in wei)
const CHANNELS_TO_DEPLOY: [(u64, [u8; 20], u128); 2] = [
	// 10.319838215944994885 ETH
	(
		7584,
		hex_literal::hex!("9d6ca7cd47c9b69173691971545446625e605ab7"),
		10_319_838_215_944_994_885,
	),
	// 9.029838215944994885 ETH
	(
		7585,
		hex_literal::hex!("5958f112ab25c46ede2ef9811410b52ea1388d90"),
		9_029_838_215_944_994_885,
	),
];

// Channels already deployed and fetched — only need balance credit.
// (address, amount in wei)
const CHANNELS_TO_CREDIT: [([u8; 20], u128); 2] = [
	// 10.689838215944994885 ETH
	(hex_literal::hex!("96d29eb80309491f0b997cc11aafa5bafb3be66a"), 10_689_838_215_944_994_885),
	// 9.199838215944994885 ETH
	(hex_literal::hex!("8354ac55fab3d4782f621acb85e90705407af613"), 9_199_838_215_944_994_885),
];

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		let owner = match AccountId32::from_ss58check(
			"cFLW4PhasdivcJKuA2BGw9Y9dz7EFwks82K8Z6U3MfCk8WcNW",
		) {
			Ok(account) => account,
			Err(err) => {
				log::info!("Failed to decode the AccountId {err:?}");
				return Weight::zero();
			},
		};

		let preallocated: BTreeSet<H160> = pallet_cf_ingress_egress::PreallocatedChannels::<
			Runtime,
			EthereumInstance,
		>::get(&owner)
		.iter()
		.map(|channel| channel.address)
		.collect();

		let already_run = CHANNELS_TO_DEPLOY.iter().any(|(channel_id, address_bytes, _)| {
			let address = H160::from(*address_bytes);
			DepositChannelLookup::<Runtime, EthereumInstance>::contains_key(address) ||
				pallet_cf_ingress_egress::DepositChannelPool::<Runtime, EthereumInstance>::contains_key(channel_id) ||
				preallocated.contains(&address)
		});
		if already_run {
			log::info!("📦 Deploy stuck ETH channels migration already applied, skipping.");
			return Weight::zero();
		}

		// Schedule recycling well after the deploy broadcast will have completed.
		// ~7200 ETH blocks ≈ 24 hours.
		let recycle_at = ProcessedUpTo::<Runtime, EthereumInstance>::get().saturating_add(7200u64);

		for (channel_id, address_bytes, amount) in CHANNELS_TO_DEPLOY {
			let address = H160::from(address_bytes);

			let deposit_channel = DepositChannel {
				channel_id,
				address,
				asset: EthAsset::Eth,
				state: DeploymentStatus::Undeployed,
			};

			let details = DepositChannelDetails {
				owner: owner.clone(),
				deposit_channel,
				opened_at: 0u64,
				expires_at: 0u64,
				action: ChannelAction::Unrefundable,
				boost_fee: 0,
				boost_status: BoostStatus::NotBoosted,
				is_marked_for_rejection: false,
			};

			DepositChannelLookup::<Runtime, EthereumInstance>::insert(address, details);

			ScheduledEgressFetchOrTransfer::<Runtime, EthereumInstance>::append(
				FetchOrTransfer::Fetch {
					asset: EthAsset::Eth,
					deposit_address: address,
					deposit_fetch_id: None,
					amount: 0,
				},
			);

			// Schedule recycling after the deploy broadcast will have completed.
			DepositChannelRecycleBlocks::<Runtime, EthereumInstance>::append((recycle_at, address));

			// Credit the stuck ETH balance to the LP account.
			pallet_cf_asset_balances::Pallet::<Runtime>::credit_account(&owner, Asset::Eth, amount);

			log::info!(
				"📦 Channel {} at {:?}: scheduled deploy and credited {} wei to LP.",
				channel_id,
				address,
				amount,
			);
		}

		// Credit balances for channels that are already deployed and fetched.
		for (address_bytes, amount) in CHANNELS_TO_CREDIT {
			let address = H160::from(address_bytes);

			pallet_cf_asset_balances::Pallet::<Runtime>::credit_account(&owner, Asset::Eth, amount);

			log::info!(
				"📦 Credited {} wei to LP for already-deployed channel at {:?}.",
				amount,
				address,
			);
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let owner =
			AccountId32::from_ss58check("cFLW4PhasdivcJKuA2BGw9Y9dz7EFwks82K8Z6U3MfCk8WcNW")
				.expect("Valid SS58");

		let preallocated = pallet_cf_ingress_egress::PreallocatedChannels::<
			Runtime,
			EthereumInstance,
		>::get(&owner);

		for (channel_id, address_bytes, _) in CHANNELS_TO_DEPLOY {
			let address = H160::from(address_bytes);
			assert!(
				!DepositChannelLookup::<Runtime, EthereumInstance>::contains_key(address),
				"Channel at {:?} already exists in DepositChannelLookup",
				address,
			);
			assert!(
				!pallet_cf_ingress_egress::DepositChannelPool::<Runtime, EthereumInstance>::contains_key(channel_id),
				"Channel {} already exists in DepositChannelPool",
				channel_id,
			);
			assert!(
				!preallocated.iter().any(|ch| ch.channel_id == channel_id),
				"Channel {} already exists in PreallocatedChannels",
				channel_id,
			);
		}
		Ok(Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		for (channel_id, address_bytes, _) in CHANNELS_TO_DEPLOY {
			let address = H160::from(address_bytes);
			let details = DepositChannelLookup::<Runtime, EthereumInstance>::get(address)
				.expect("Channel should exist after migration");
			assert_eq!(details.deposit_channel.channel_id, channel_id);
			assert_eq!(details.deposit_channel.state, DeploymentStatus::Undeployed);
		}

		let scheduled = ScheduledEgressFetchOrTransfer::<Runtime, EthereumInstance>::get();
		let scheduled_addresses: Vec<_> = scheduled
			.iter()
			.filter_map(|f| match f {
				FetchOrTransfer::Fetch { deposit_address, .. } => Some(*deposit_address),
				_ => None,
			})
			.collect();

		for (_, address_bytes, _) in CHANNELS_TO_DEPLOY {
			let address = H160::from(address_bytes);
			assert!(
				scheduled_addresses.contains(&address),
				"Fetch for {:?} not found in ScheduledEgressFetchOrTransfer",
				address,
			);
		}

		Ok(())
	}
}
