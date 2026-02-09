use cf_chains::instances::PalletInstanceAlias;
use cf_primitives::BlockWitnesserEvent;
use cf_traits::Hook;
use cf_utilities::{define_empty_struct, derive_common_traits_no_bounds, hook_impls};
use generic_typeinfo_derive::GenericTypeInfo;
use pallet_cf_broadcast::TransactionConfirmation;
use pallet_cf_ingress_egress::{
	DepositWitness, TargetChainBlockNumber, TransferFailedWitness, VaultDepositWitness,
};
use scale_info::TypeInfo;
use sp_std::boxed::Box;

// open up this enum since it's used in many matches
use BlockWitnesserEvent::*;

trait Config<I: 'static> = pallet_cf_ingress_egress::Config<I>
	+ pallet_cf_vaults::Config<I>
	+ pallet_cf_broadcast::Config<I>;

type VaultDepositInput<T, I> =
	(BlockWitnesserEvent<VaultDepositWitness<T, I>>, TargetChainBlockNumber<T, I>);
type VaultTransferFailedInput<T, I> =
	(BlockWitnesserEvent<TransferFailedWitness<T, I>>, TargetChainBlockNumber<T, I>);

define_empty_struct! {
	pub struct PalletHooks<T: Config<I>, I: 'static>;
}

hook_impls! {
	for PalletHooks<T, I> where (T: Config<I>, I: 'static):

	// --- deposit channel witnessing dispatch ---
	fn(&mut self, (event, block_height): (BlockWitnesserEvent<DepositWitness<T::TargetChain>>, TargetChainBlockNumber<T, I>)) -> () {
		match event {
			PreWitness(deposit_witness) => {
				let _ = pallet_cf_ingress_egress::Pallet::<T, I>::process_channel_deposit_prewitness(
					deposit_witness,
					block_height,
				);
			},
			Witness(deposit_witness) => {
				pallet_cf_ingress_egress::Pallet::<T, I>::process_channel_deposit_full_witness(deposit_witness, block_height);
			},
		}
	}

	// --- vault swap witnessing dispatch ---
	fn(&mut self, (event, block_height): (BlockWitnesserEvent<VaultDepositWitness<T, I>>, TargetChainBlockNumber<T, I>)) -> () {
		match event {
			PreWitness(deposit) => {
				pallet_cf_ingress_egress::Pallet::<T, I>::process_vault_swap_request_prewitness(
					block_height,
					deposit.clone(),
				);
			},
			Witness(deposit) => {
				pallet_cf_ingress_egress::Pallet::<T, I>::process_vault_swap_request_full_witness_inner(
					block_height,
					deposit.clone(),
				);
			},
		}
	}

	// --- egress witnessing dispatch ---
	fn(&mut self, (event, block_height): (BlockWitnesserEvent<TransactionConfirmation<T, I>>, TargetChainBlockNumber<T, I>)) -> ()
	where (
		<T as frame_system::Config>::RuntimeOrigin: From<pallet_cf_witnesser::RawOrigin>,
	)
	{
		match event {
			PreWitness(_) => { /* We don't care about pre-witnessing an egress */
			},
			Witness(egress) => {
				if let Err(err) = pallet_cf_broadcast::Pallet::<T, I>::egress_success(
					pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold.into(),
					egress.clone(),
					block_height,
				) {
					log::error!(
						"Failed to execute Bitcoin egress success: TxOutId: {:?}, Error: {:?}",
						egress.tx_out_id,
						err
					)
				}
			},
		}
	}

	// -- transfer failed dispatch --
	fn(&mut self, (event, _block_height): (BlockWitnesserEvent<TransferFailedWitness<T, I>>, TargetChainBlockNumber<T, I>)) -> () {
		match event {
			PreWitness(_) => { /* We don't care about pre-witnessing a failure */ },
			Witness(failure) => pallet_cf_ingress_egress::Pallet::<T, I>::vault_transfer_failed_inner(failure),
		}
	}

	// --- evm vault contract events (either vault swaps OR transfer failures)
	fn(&mut self, (event, block_height): (BlockWitnesserEvent<VaultContractEvent<T, I>>, TargetChainBlockNumber<T, I>)) -> () {
		use VaultContractEvent::*;
		match event {
			// vault deposits
			PreWitness(VaultDeposit(deposit)) => <Self as Hook<(VaultDepositInput<T, I>, ())>>::run(self, (PreWitness(*deposit), block_height)),
			Witness(VaultDeposit(deposit)) => <Self as Hook<(VaultDepositInput<T, I>, ())>>::run(self, (Witness(*deposit), block_height)),

			// failures
			PreWitness(TransferFailed(witness)) => <Self as Hook<(VaultTransferFailedInput<T, I>, ())>>::run(self, (PreWitness(witness), block_height)),
			Witness(TransferFailed(witness)) => <Self as Hook<(VaultTransferFailedInput<T, I>, ())>>::run(self, (Witness(witness), block_height)),
		}
	}
}

derive_common_traits_no_bounds! {
	#[derive_where(PartialOrd, Ord; )]
	#[derive(GenericTypeInfo)]
	#[expand_name_with(<T::TargetChain as PalletInstanceAlias>::TYPE_INFO_SUFFIX)]
	pub enum VaultContractEvent<T: pallet_cf_ingress_egress::Config<I>, I: 'static> {
		VaultDeposit(Box<VaultDepositWitness<T, I>>),
		TransferFailed(TransferFailedWitness<T, I>)
	}
}
