use crate::*;
use frame_support::traits::OnRuntimeUpgrade;

mod old {

	use super::*;

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum ChannelAction<AccountId> {
		Swap {
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			broker_fees: Beneficiaries<AccountId>,
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

	#[derive(CloneNoBound, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub struct DepositChannelDetails<T: Config<I>, I: 'static> {
		pub deposit_channel: DepositChannel<T::TargetChain>,
		/// The block number at which the deposit channel was opened, expressed as a block number
		/// on the external Chain.
		pub opened_at: TargetChainBlockNumber<T, I>,
		/// The last block on the target chain that the witnessing will witness it in. If funds are
		/// sent after this block, they will not be witnessed.
		pub expires_at: TargetChainBlockNumber<T, I>,

		/// The action to be taken when the DepositChannel is deposited to.
		pub action: ChannelAction<T::AccountId>,
		/// The boost fee
		pub boost_fee: BasisPoints,
		/// Boost status, indicating whether there is pending boost on the channel
		pub boost_status: BoostStatus<TargetChainAmount<T, I>>,
	}

	#[frame_support::storage_alias]
	pub type DepositChannelLookup<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		TargetChainAccount<T, I>,
		DepositChannelDetails<T, I>,
		OptionQuery,
	>;
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		DepositChannelLookup::<T, I>::translate(|_, details: old::DepositChannelDetails<T, I>| {
			Some(DepositChannelDetails {
				deposit_channel: details.deposit_channel,
				opened_at: details.opened_at,
				expires_at: details.expires_at,
				action: match details.action {
					old::ChannelAction::Swap {
						destination_asset,
						destination_address,
						broker_fees,
					} => ChannelAction::Swap {
						destination_asset,
						destination_address,
						broker_fees,
						refund_params: None,
					},
					old::ChannelAction::LiquidityProvision { lp_account } =>
						ChannelAction::LiquidityProvision { lp_account },
					old::ChannelAction::CcmTransfer {
						destination_asset,
						destination_address,
						channel_metadata,
					} => ChannelAction::CcmTransfer {
						destination_asset,
						destination_address,
						channel_metadata,
						refund_params: None,
					},
				},
				boost_fee: details.boost_fee,
				boost_status: details.boost_status,
			})
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}

#[cfg(test)]
mod tests {

	use super::*;

	use crate::mock_btc::{new_test_ext, Test};
	use cf_chains::{
		btc::{
			deposit_address::{DepositAddress, TapscriptPath},
			BitcoinScript, ScriptPubkey,
		},
		Bitcoin,
	};

	fn mock_deposit_channel() -> DepositChannel<Bitcoin> {
		DepositChannel {
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
		}
	}

	#[test]
	fn test_migration() {
		new_test_ext().execute_with(|| {
			let input_address_1 = ScriptPubkey::Taproot([0u8; 32]);
			let input_address_2 = ScriptPubkey::Taproot([1u8; 32]);
			let output_address = ForeignChainAddress::Eth([0u8; 20].into());

			let old_details_swap = old::DepositChannelDetails::<Test, _> {
				deposit_channel: mock_deposit_channel(),
				opened_at: Default::default(),
				expires_at: Default::default(),
				boost_status: BoostStatus::NotBoosted,
				action: old::ChannelAction::Swap {
					destination_asset: Asset::Flip,
					destination_address: output_address.clone(),
					broker_fees: Default::default(),
				},
				boost_fee: 0,
			};

			let old_details_ccm = old::DepositChannelDetails::<Test, _> {
				action: old::ChannelAction::CcmTransfer {
					destination_asset: Asset::Flip,
					destination_address: output_address.clone(),
					channel_metadata: CcmChannelMetadata {
						message: vec![0u8, 1u8, 2u8, 3u8, 4u8].try_into().unwrap(),
						gas_budget: 50 * 10u128.pow(18),
						cf_parameters: Default::default(),
					},
				},
				..old_details_swap.clone()
			};

			old::DepositChannelLookup::<Test, ()>::insert(
				input_address_1.clone(),
				old_details_swap,
			);

			old::DepositChannelLookup::<Test, ()>::insert(input_address_2.clone(), old_details_ccm);

			Migration::<Test, ()>::on_runtime_upgrade();

			assert_eq!(
				DepositChannelLookup::<Test, ()>::get(input_address_1),
				Some(DepositChannelDetails::<Test, _> {
					deposit_channel: mock_deposit_channel(),
					opened_at: Default::default(),
					expires_at: Default::default(),
					boost_status: BoostStatus::NotBoosted,
					action: ChannelAction::Swap {
						destination_asset: Asset::Flip,
						destination_address: output_address.clone(),
						broker_fees: Default::default(),
						refund_params: None,
					},
					boost_fee: 0,
				})
			);
			assert_eq!(
				DepositChannelLookup::<Test, ()>::get(input_address_2),
				Some(DepositChannelDetails::<Test, _> {
					deposit_channel: mock_deposit_channel(),
					opened_at: Default::default(),
					expires_at: Default::default(),
					boost_status: BoostStatus::NotBoosted,
					action: ChannelAction::CcmTransfer {
						destination_asset: Asset::Flip,
						destination_address: output_address.clone(),
						channel_metadata: CcmChannelMetadata {
							message: vec![0u8, 1u8, 2u8, 3u8, 4u8].try_into().unwrap(),
							gas_budget: 50 * 10u128.pow(18),
							cf_parameters: Default::default(),
						},
						refund_params: None,
					},
					boost_fee: 0,
				})
			);

			dbg!();
		});
	}
}
