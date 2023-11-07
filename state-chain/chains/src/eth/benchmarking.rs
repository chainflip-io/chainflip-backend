#![cfg(feature = "runtime-benchmarks")]

use cf_primitives::chains::assets::eth::Asset;

use crate::{
	benchmarking_value::{BenchmarkValue, BenchmarkValueExtended},
	evm::api::{EvmReplayProtection, EvmTransactionBuilder},
};

use super::{
	api::{update_flip_supply::UpdateFlipSupply, EthereumApi},
	deposit_address::EthereumDepositChannel,
	EthereumTrackedData,
};

impl<E> BenchmarkValue for EthereumApi<E> {
	fn benchmark_value() -> Self {
		EvmTransactionBuilder::new_unsigned(
			EvmReplayProtection::default(),
			UpdateFlipSupply::new(1000000u128, 1u64),
		)
		.into()
	}
}

impl BenchmarkValue for EthereumTrackedData {
	fn benchmark_value() -> Self {
		Self { base_fee: 10_000_000_000, priority_fee: 2_000_000_000 }
	}
}

impl BenchmarkValueExtended for EthereumDepositChannel {
	fn benchmark_value_by_id(id: u8) -> Self {
		Self {
			channel_id: id.into(),
			address: ethereum_types::H160::repeat_byte(id),
			asset: Asset::Eth,
			deployment_status: super::DeploymentStatus::Undeployed,
		}
	}
}
