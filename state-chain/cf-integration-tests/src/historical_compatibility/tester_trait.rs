// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;

use cf_utilities::migrations::basics::{HasGenericVariant, HasVersion, Version};
use codec::{Decode, Encode};
use proptest::{
	arbitrary::Arbitrary,
	prelude::TestCaseError,
	strategy::Strategy,
	test_runner::{Config, TestRunner},
};
use scale_info::TypeInfo;
use similar::{ChangeTag, TextDiff};

pub trait HistoricalCompatibilityTester {
	fn test_call<
		V: Version,
		I: HasVersion<V, HistoricalType: Encode + std::fmt::Debug + TypeInfo + 'static + Arbitrary>
			+ HasGenericVariant<GenericType: Arbitrary>,
		O: HasVersion<
				V,
				HistoricalType: Encode + Decode + TypeInfo + std::fmt::Debug + 'static + Arbitrary,
			> + HasGenericVariant<GenericType: Arbitrary>,
	>(
		&mut self,
		version: V,
		api_name: &'static str,
		method_name: &'static str,
	) -> Vec<TypeIncompatibilityInfo>;
}

#[derive(Clone, Copy)]
pub enum TypeRef {
	RuntimeCall { api_name: &'static str, method_name: &'static str, version: u32 },
}

impl std::fmt::Display for TypeRef {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			TypeRef::RuntimeCall { api_name, method_name, version } =>
				write!(f, "`{method_name}` @ v{version} ({api_name})"),
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
	pub type_diff: TypeDiff,
}

#[derive(Hash, PartialEq, Eq, Clone)]
pub struct TypeDiff {
	pub actual_encoding: String,
	pub expected_encoding: String,
}

impl TypeDiff {
	pub fn get_summary(&self) -> TypeDiffSummary {
		let diff = TextDiff::from_lines(&self.actual_encoding, &self.expected_encoding);

		let mut inserts: Vec<Vec<String>> = Vec::new();
		let mut deletions: Vec<Vec<String>> = Vec::new();
		let mut last_tag = None;

		for change in diff.iter_all_changes() {
			let line = change.to_string_lossy().trim_end().to_string();
			match change.tag() {
				ChangeTag::Insert => {
					if last_tag != Some(ChangeTag::Insert) {
						inserts.push(Vec::new());
					}
					inserts.last_mut().unwrap().push(line);
				},
				ChangeTag::Delete => {
					if last_tag != Some(ChangeTag::Delete) {
						deletions.push(Vec::new());
					}
					deletions.last_mut().unwrap().push(line);
				},
				ChangeTag::Equal => {},
			}
			last_tag = Some(change.tag());
		}

		TypeDiffSummary { inserts, deletions }
	}
}

pub struct TypeDiffSummary {
	pub inserts: Vec<Vec<String>>,
	pub deletions: Vec<Vec<String>>,
}

const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const RESET: &str = "\x1b[0m";

impl std::fmt::Display for TypeDiffSummary {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		fn indentation(s: &str) -> usize {
			s.len() - s.trim_start().len()
		}

		fn write_top_level_lines(
			f: &mut std::fmt::Formatter<'_>,
			groups: &[Vec<String>],
			prefix: &str,
			color: &str,
			reset: &str,
		) -> std::fmt::Result {
			for group in groups {
				let top_indent = group.first().map(|l| indentation(l));
				for line in group {
					if Some(indentation(line)) == top_indent {
						writeln!(f, "    {color}{prefix} {}{reset}", line.trim_start())?;
					}
				}
			}
			Ok(())
		}

		if !self.inserts.is_empty() {
			write_top_level_lines(f, &self.inserts, "+", GREEN, RESET)?;
		}
		if !self.deletions.is_empty() {
			write_top_level_lines(f, &self.deletions, "-", RED, RESET)?;
		}
		Ok(())
	}
}

impl std::fmt::Display for TypeDiff {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let diff = TextDiff::from_lines(&self.actual_encoding, &self.expected_encoding);

		for change in diff.iter_all_changes() {
			let (sign, color) = match change.tag() {
				ChangeTag::Delete => ("-", RED),
				ChangeTag::Insert => ("+", GREEN),
				ChangeTag::Equal => (" ", ""),
			};
			if color.is_empty() {
				write!(f, " {change}")?;
			} else {
				write!(f, "{color}{sign}{change}{RESET}")?;
			}
		}

		Ok(())
	}
}

pub struct SubTypeIncompatibility {
	pub sub_type_details: SubTypeDetails,
	pub error: String,
}

pub fn fuzzy_test_encode_decode_compatibility<T1: Encode>(
	cases: u32,
	strategy: &impl Strategy<Value = T1>,
	encode: &impl Fn(T1) -> Result<Vec<u8>, SubTypeIncompatibility>,
	decode: &impl Fn(&mut &[u8]) -> Result<(), SubTypeIncompatibility>,
	type_details: SubTypeDetails,
) -> Result<(), SubTypeIncompatibility> {
	let mut runner =
		TestRunner::new(Config { failure_persistence: None, cases, ..Default::default() });

	let incompatibility = RefCell::new(None);

	runner
		.run(strategy, |value1| {
			encode(value1)
				.and_then(|encoded| {
					let mut slice: &[u8] = &encoded;
					let cursor: &mut &[u8] = &mut slice;
					decode(cursor).and_then(|_| {
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
