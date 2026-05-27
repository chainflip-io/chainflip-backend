#![cfg(test)]

use cf_utilities::migrations::basics::{HasGenericVariant, HasVersion, VariantName};
use codec::{Decode, Encode};
use proptest::arbitrary::Arbitrary;
use scale_info::TypeInfo;

pub trait HistoricalCompatibilityTester {
	fn test_call<
		V: VariantName,
		I: std::fmt::Debug
			+ HasVersion<V, HistoricalType: Encode + std::fmt::Debug + TypeInfo + 'static>
			+ HasGenericVariant<GenericType: Arbitrary>,
		O: std::fmt::Debug
			+ HasVersion<V, HistoricalType: Encode + Decode + TypeInfo + 'static + std::fmt::Debug>
			+ HasGenericVariant<GenericType: Arbitrary>,
	>(
		&mut self,
		version: V,
		api_name: &'static str,
		method_name: &'static str,
		file_path: &'static str,
	);
}
