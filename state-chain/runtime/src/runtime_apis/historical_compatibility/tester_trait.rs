#![cfg(test)]

use std::{cell::RefCell, path::Display};

use cf_utilities::migrations::basics::{HasGenericVariant, HasVersion, VariantName};
use codec::{Decode, Encode};
use proptest::{
	arbitrary::Arbitrary,
	prelude::TestCaseError,
	strategy::Strategy,
	test_runner::{Config, FileFailurePersistence, TestRunner},
};
use scale_info::TypeInfo;

pub trait HistoricalCompatibilityTester {
	fn test_call<
		V: VariantName,
		I: std::fmt::Debug
			+ HasVersion<V, HistoricalType: Encode + std::fmt::Debug + TypeInfo + 'static + Arbitrary>
			+ HasGenericVariant<GenericType: Arbitrary>,
		O: std::fmt::Debug
			+ HasVersion<
				V,
				HistoricalType: Encode + Decode + TypeInfo + 'static + std::fmt::Debug + Arbitrary,
			> + HasGenericVariant<GenericType: Arbitrary>,
	>(
		&mut self,
		version: V,
		api_name: &'static str,
		method_name: &'static str,
		file_path: &'static str,
	) -> Vec<TypeIncompatibilityInfo>;
}

pub enum ArgPos {
	Input(u32),
	Output(),
}

#[derive(Clone, Copy)]
pub enum TypeRef {
	RuntimeCall { api_name: &'static str, method_name: &'static str },
}

impl std::fmt::Display for TypeRef {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			TypeRef::RuntimeCall { api_name, method_name } =>
				write!(f, "`{method_name}` ({api_name})"),
		}
	}
}

#[derive(PartialEq, Eq, Hash, Clone)]
pub enum TypeName {
	Named { name: Option<String> },
	InputArgumentList,
}

impl std::fmt::Display for TypeName {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			TypeName::Named { name: Some(name) } => write!(f, "{name}"),
			TypeName::Named { name: None } => write!(f, "<anonymous>"),
			TypeName::InputArgumentList => write!(f, "function arguments"),
		}
	}
}

#[derive(Clone)]
pub struct SubTypeDetails {
	pub type_name: TypeName,
	pub location: SubTypeLocation,
}

#[derive(Clone)]
pub struct FullTypeLocation {
	pub reference: TypeRef,
	pub sub_location: SubTypeLocation,
}

impl std::fmt::Display for FullTypeLocation {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{} of {}", self.sub_location, self.reference)
	}
}

#[derive(Clone, Copy)]
pub enum SubTypeLocation {
	Input { pos: Option<u32> },
	Output,
}

impl std::fmt::Display for SubTypeLocation {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			SubTypeLocation::Input { pos: Some(pos) } => write!(f, "arg#{pos}"),
			SubTypeLocation::Input { pos: None } => write!(f, "args"),
			SubTypeLocation::Output => write!(f, "result"),
		}
	}
}

pub struct TypeIncompatibilityInfo {
	pub sub_type_incompat: SubTypeIncompatibility,
	pub type_ref: TypeRef,
	pub expected_encoding: String,
	pub actual_encoding: String,
	pub type_name: Option<String>,
}

impl std::fmt::Display for TypeIncompatibilityInfo {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		use similar::{ChangeTag, TextDiff};

		writeln!(f, "Type is incompatible")?;
		writeln!(f, "Occurs in: {}", self.type_ref)?;

		let diff =
			TextDiff::from_lines(self.actual_encoding.clone(), self.expected_encoding.clone());

		for change in diff.iter_all_changes() {
			let sign = match change.tag() {
				ChangeTag::Delete => "-",
				ChangeTag::Insert => "+",
				ChangeTag::Equal => " ",
			};
			write!(f, "{}{}", sign, change)?;
		}

		Ok(())
	}
}

pub struct SubTypeIncompatibility {
	pub sub_type_details: SubTypeDetails,
	pub error: String,
}

pub fn fuzzy_test_encode_decode_compatibility<T1: Encode>(
	file_path: &'static str,
	strategy: &impl Strategy<Value = T1>,
	encode: &impl Fn(T1) -> Vec<u8>,
	decode: &impl Fn(&[u8]) -> Result<(), SubTypeIncompatibility>,
	type_details: SubTypeDetails,
) -> Result<(), SubTypeIncompatibility> {
	let mut runner = TestRunner::new(Config {
		source_file: Some(file_path),
		failure_persistence: None,
		cases: 200,
		..Default::default()
	});

	let incompatibility = RefCell::new(None);

	runner
		.run(strategy, |value1| {
			let mut cursor = &encode(value1);
			decode(&mut cursor)
				.and_then(|_| {
					if cursor.is_empty() {
						Ok(())
					} else {
						Err(SubTypeIncompatibility {
							sub_type_details: type_details.clone(),
							error: format!(
								"Encoding mismatch: {} trailing bytes remain after decoding",
								cursor.len(),
							),
						})
					}
				})
				.map_err(|err| {
					incompatibility.replace(Some(err));
					TestCaseError::Fail("".into())
				})
		})
		.map_err(|err| match err {
			proptest::test_runner::TestError::Abort(reason) =>
				panic!("Proptest aborted because: {reason}"),
			proptest::test_runner::TestError::Fail(_reason, _) =>
				incompatibility.take().expect("proptest failed but no error was recorded"),
		})
}
