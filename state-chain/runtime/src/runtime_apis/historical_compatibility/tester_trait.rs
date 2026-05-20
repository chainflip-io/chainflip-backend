#![cfg(test)]

use cf_utilities::migrations::basics::{HasVersion, VariantName};
use codec::{Decode, Encode};
use proptest::arbitrary::Arbitrary;

pub trait HistoricalCompatibilityTester {
	fn test_call<
		V: VariantName,
		I: Arbitrary + std::fmt::Debug + HasVersion<V, HistoricalType: Encode>,
		O: Arbitrary + std::fmt::Debug + HasVersion<V, HistoricalType: Encode + Decode>,
	>(
		&mut self,
		version: V,
		api_name: &'static str,
		method_name: &'static str,
		file_path: &'static str,
	);
}
