use crate::*;
use cf_chains::DepositChannel;
use cf_primitives::Beneficiary;
use frame_support::traits::OnRuntimeUpgrade;

mod old {
	use super::*;

	#[derive(CloneNoBound, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub struct DepositChannelDetails<T: Config<I>, I: 'static = ()> {
		pub deposit_channel: DepositChannel<T::TargetChain>,
		/// The block number at which the deposit channel was opened, expressed as a block number
		/// on the external Chain.
		pub opened_at: TargetChainBlockNumber<T, I>,
		/// The last block on the target chain that the witnessing will witness it in. If funds are
		/// sent after this block, they will not be witnessed.
		pub expires_at: TargetChainBlockNumber<T, I>,

		/// The action to be taken when the DepositChannel is deposited to.
		pub action: old::ChannelAction<T::AccountId>,
		/// The boost fee
		pub boost_fee: BasisPoints,
		/// Boost status, indicating whether there is pending boost on the channel
		pub boost_status: BoostStatus,
	}

	#[frame_support::storage_alias]
	pub type DepositChannelLookup<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		TargetChainAccount<T, I>,
		old::DepositChannelDetails<T, I>,
		OptionQuery,
	>;

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum ChannelAction<AccountId> {
		Swap {
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			broker_id: AccountId,
			broker_commission_bps: BasisPoints,
		},
		LiquidityProvision {
			lp_account: AccountId,
		},
		CcmTransfer {
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			channel_metadata: CcmChannelMetadata,
		},
	}
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		DepositChannelLookup::<T, I>::translate(
			|_address, old_channel: old::DepositChannelDetails<T, I>| match old_channel.action {
				old::ChannelAction::Swap {
					destination_asset,
					destination_address,
					broker_id,
					broker_commission_bps,
				} => Some(DepositChannelDetails::<T, I> {
					deposit_channel: old_channel.deposit_channel,
					opened_at: old_channel.opened_at,
					expires_at: old_channel.expires_at,
					action: ChannelAction::Swap {
						destination_asset,
						destination_address,
						broker_fees: vec![Beneficiary {
							account: broker_id,
							bps: broker_commission_bps,
						}]
						.try_into()
						.unwrap(),
					},
					boost_fee: old_channel.boost_fee,
					boost_status: old_channel.boost_status,
				}),
				old::ChannelAction::LiquidityProvision { lp_account } =>
					Some(DepositChannelDetails::<T, I> {
						deposit_channel: old_channel.deposit_channel,
						opened_at: old_channel.opened_at,
						expires_at: old_channel.expires_at,
						action: ChannelAction::LiquidityProvision { lp_account },
						boost_fee: old_channel.boost_fee,
						boost_status: old_channel.boost_status,
					}),
				old::ChannelAction::CcmTransfer {
					destination_asset,
					destination_address,
					channel_metadata,
				} => Some(DepositChannelDetails::<T, I> {
					deposit_channel: old_channel.deposit_channel,
					opened_at: old_channel.opened_at,
					expires_at: old_channel.expires_at,
					action: ChannelAction::CcmTransfer {
						destination_asset,
						destination_address,
						channel_metadata,
					},
					boost_fee: old_channel.boost_fee,
					boost_status: old_channel.boost_status,
				}),
			},
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let number_of_channels_in_lookup =
			old::DepositChannelLookup::<T, I>::iter_keys().count() as u32;

		Ok(number_of_channels_in_lookup.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let number_of_channels_in_lookup_pre_migration =
			<u32>::decode(&mut &state[..]).expect("Pre-migration should encode a u32.");
		ensure!(
			DepositChannelLookup::<T, I>::iter_keys().count() as u32 ==
				number_of_channels_in_lookup_pre_migration,
			"DepositChannelLookup migration failed."
		);
		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {
	use sp_core::H160;

	#[test]
	fn test_migration() {
		use super::*;
		use crate::mock_btc::*;
		use cf_chains::btc::{
			deposit_address::{DepositAddress, TapscriptPath},
			BitcoinScript, ScriptPubkey,
		};

		new_test_ext().execute_with(|| {
			let address1 = ScriptPubkey::Taproot([0u8; 32]);
			let address2 = ScriptPubkey::Taproot([1u8; 32]);
			let broker_address = 10u64;
			let mock_deposit_channel_details = || -> old::DepositChannelDetails<Test> {
				old::DepositChannelDetails::<Test, _> {
					deposit_channel: DepositChannel {
						channel_id: 123,
						address: ScriptPubkey::Taproot([0u8; 32]).clone(),
						asset: <Bitcoin as Chain>::ChainAsset::Btc,
						state: DepositAddress {
							pubkey_x: [1u8; 32],
							script_path: Some(TapscriptPath {
								salt: 123,
								tweaked_pubkey_bytes: [2u8; 33],
								tapleaf_hash: [3u8; 32],
								unlock_script: BitcoinScript::new(Default::default()),
							}),
						},
					},
					opened_at: Default::default(),
					expires_at: Default::default(),
					action: old::ChannelAction::Swap {
						destination_asset: Asset::Eth,
						destination_address: ForeignChainAddress::Eth(H160([5u8; 20])),
						broker_id: broker_address,
						broker_commission_bps: 15,
					},
					boost_fee: 0,
					boost_status: BoostStatus::NotBoosted,
				}
			};

			// Insert mock data into old storage
			old::DepositChannelLookup::insert(address1.clone(), mock_deposit_channel_details());
			old::DepositChannelLookup::insert(address2.clone(), mock_deposit_channel_details());

			#[cfg(feature = "try-runtime")]
			let state: Vec<u8> = super::Migration::<Test, _>::pre_upgrade().unwrap();

			// Perform runtime migration.
			super::Migration::<Test, _>::on_runtime_upgrade();

			#[cfg(feature = "try-runtime")]
			super::Migration::<Test, _>::post_upgrade(state).unwrap();

			// Verify data is correctly migrated into new storage.
			for address in [address1, address2] {
				let channel = DepositChannelLookup::<Test>::get(address);
				assert!(channel.is_some());
				assert_eq!(
					channel.unwrap().action,
					ChannelAction::Swap {
						destination_asset: Asset::Eth,
						destination_address: ForeignChainAddress::Eth(H160([5u8; 20])),
						broker_fees: vec![Beneficiary { account: broker_address, bps: 15 }]
							.try_into()
							.unwrap()
					}
				);
			}
		});
	}
}
