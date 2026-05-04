use crate::*;
use frame_support::traits::UncheckedOnRuntimeUpgrade;

pub struct Migration<T: Config>(PhantomData<T>);

mod old {
	use super::*;
	use crate::general_lending::config::{
		LendingPoolConfiguration, LtvThresholds, NetworkFeeContributions,
	};

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct LendingConfiguration {
		pub default_pool_config: LendingPoolConfiguration,
		pub ltv_thresholds: LtvThresholds,
		pub network_fee_contributions: NetworkFeeContributions,
		pub fee_swap_interval_blocks: u32,
		pub interest_payment_interval_blocks: u32,
		pub interest_collection_threshold_usd: AssetAmount,
		pub fee_swap_threshold_usd: AssetAmount,
		pub soft_liquidation_max_oracle_slippage: BasisPoints,
		pub hard_liquidation_max_oracle_slippage: BasisPoints,
		pub soft_liquidation_swap_chunk_size_usd: AssetAmount,
		pub hard_liquidation_swap_chunk_size_usd: AssetAmount,
		pub fee_swap_max_oracle_slippage: BasisPoints,
		pub pool_config_overrides: BTreeMap<Asset, LendingPoolConfiguration>,
		pub minimum_loan_amount_usd: AssetAmount,
		pub minimum_supply_amount_usd: AssetAmount,
		pub minimum_update_loan_amount_usd: AssetAmount,
		pub minimum_update_supply_amount_usd: AssetAmount,
	}
}

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		// Reads the on-chain `LendingConfig` value as the old (pre-`liquidation_coverage_factor`)
		// layout and rewrites it with the new field defaulted to 100%, matching
		// `LENDING_DEFAULT_CONFIG`. If decoding the old layout fails (e.g. on a fresh chain
		// where no value has been written), we leave the storage empty so subsequent reads
		// fall back to `LendingConfigDefault`, which already includes the new field.
		let translate_result = LendingConfig::<T>::translate::<old::LendingConfiguration, _>(
			|maybe_old: Option<old::LendingConfiguration>| {
				maybe_old.map(|old| LendingConfiguration {
					default_pool_config: old.default_pool_config,
					ltv_thresholds: old.ltv_thresholds,
					network_fee_contributions: old.network_fee_contributions,
					fee_swap_interval_blocks: old.fee_swap_interval_blocks,
					interest_payment_interval_blocks: old.interest_payment_interval_blocks,
					interest_collection_threshold_usd: old.interest_collection_threshold_usd,
					fee_swap_threshold_usd: old.fee_swap_threshold_usd,
					soft_liquidation_max_oracle_slippage: old.soft_liquidation_max_oracle_slippage,
					hard_liquidation_max_oracle_slippage: old.hard_liquidation_max_oracle_slippage,
					soft_liquidation_swap_chunk_size_usd: old.soft_liquidation_swap_chunk_size_usd,
					hard_liquidation_swap_chunk_size_usd: old.hard_liquidation_swap_chunk_size_usd,
					fee_swap_max_oracle_slippage: old.fee_swap_max_oracle_slippage,
					pool_config_overrides: old.pool_config_overrides,
					minimum_loan_amount_usd: old.minimum_loan_amount_usd,
					minimum_supply_amount_usd: old.minimum_supply_amount_usd,
					minimum_update_loan_amount_usd: old.minimum_update_loan_amount_usd,
					minimum_update_supply_amount_usd: old.minimum_update_supply_amount_usd,
					liquidation_coverage_factor: Percent::from_percent(100),
				})
			},
		);

		if let Err(e) = translate_result {
			log::error!("Failed to migrate LendingConfig: {:?}", e);
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		ensure!(
			LendingConfig::<T>::get().liquidation_coverage_factor == Percent::from_percent(100),
			"liquidation_coverage_factor should be 100% after migration"
		);
		Ok(())
	}
}
