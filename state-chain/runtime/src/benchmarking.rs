use cf_chains::eth::update_flip_supply::UpdateFlipSupply;
use cf_runtime_benchmark_utilities::BenchmarkDefault;

/// A dummy instance of RegisterClaim for benchmarking the threshld siging pallet.
impl BenchmarkDefault for crate::chainflip::EthereumSigningContext {
	fn benchmark_default() -> Self {
		Self::UpdateFlipSupply(UpdateFlipSupply::new_unsigned(
			0,
			90_000_100_000_000_000_000_000_000u128,
			800,
		))
	}
}
