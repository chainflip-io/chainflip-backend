use cf_runtime_benchmark_utilities::BenchmarkDefault;

/// A dummy instance of RegisterClaim for benchmarking the threshld siging pallet.
impl cf_runtime_benchmark_utilities::BenchmarkDefault for EthereumSigningContext {
	fn benchmark_default() -> Self {
		Self::PostClaimSignature(RegisterClaim::new_unsigned(
			0,
			&[0xcf; 32],
			12_000,
			&[0xe4; 20],
			3600 * 48,
		))
	}
}
