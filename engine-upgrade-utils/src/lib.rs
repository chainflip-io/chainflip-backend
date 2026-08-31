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

use std::{
	ffi::{c_void, CString},
	mem::size_of,
};

pub mod build_helpers;

// !!!!!!! These constants are used to check the versions across several crates and build scripts.
// These should be the first things changed when bumping the version, as it will check the
// rest of the places the version needs changing on build using the build scripts in each of the
// relevant crates.
// Should also check that the compatibility function below `args_compatible_with_old` is correct.
pub const OLD_VERSION: &str = "2.2.8";
pub const NEW_VERSION: &str = "2.3.0";

pub const ENGINE_LIB_PREFIX: &str = "chainflip_engine_v";
pub const ENGINE_ENTRYPOINT_PREFIX: &str = "cfe_entrypoint_v";

// Sometimes we need to adapt arguments between the new and old versions while both CFEs can be run
// by the upgrade runner. Arguments unsupported by the old engine are listed below and filtered so
// the fallback engine can still parse the command line.
struct IncompatibleArg {
	name: &'static str,
	takes_value: bool,
}

/// Arguments that OLD_VERSION cannot parse.
///
/// This list must be reviewed whenever OLD_VERSION or NEW_VERSION changes so it
/// reflects only the CLI differences between the two engine versions.
const INCOMPATIBLE_WITH_OLD: &[IncompatibleArg] = &[
	IncompatibleArg { name: "--bsc.rpc.http_endpoint", takes_value: true },
	IncompatibleArg { name: "--bsc.backup_rpc.http_endpoint", takes_value: true },
	IncompatibleArg { name: "--bsc.private_key_file", takes_value: true },
];

fn filter_incompatible_args(
	args: Vec<String>,
	incompatible_args: &[IncompatibleArg],
) -> Vec<String> {
	let mut args = args.into_iter();
	let mut compatible_args = Vec::new();

	while let Some(arg) = args.next() {
		let (name, has_inline_value) = match arg.split_once('=') {
			Some((name, _)) => (name, true),
			None => (arg.as_str(), false),
		};

		if let Some(incompatible_arg) =
			incompatible_args.iter().find(|incompatible_arg| incompatible_arg.name == name)
		{
			if incompatible_arg.takes_value && !has_inline_value {
				// The new engine has already parsed these arguments before fallback,
				// so a split value-taking option is known to have a following value.
				let _ = args.next();
			}

			continue;
		}

		compatible_args.push(arg);
	}

	compatible_args
}

pub fn args_compatible_with_old(args: Vec<String>) -> Vec<String> {
	filter_incompatible_args(args, INCOMPATIBLE_WITH_OLD)
}

pub use std::ffi::c_char;
pub const NO_START_FROM: u32 = 0;

// ====  Status codes ====
pub const SUCCESS: i32 = 0;
pub const PANIC: i32 = -1;
pub const UNKNOWN_ERROR: i32 = -2;
pub const ERROR_READING_SETTINGS: i32 = -3;
/// The version of the engine is no longer compatible with the runtime.
pub const NO_LONGER_COMPATIBLE: i32 = 1;
/// The engine is not yet compatible with the runtime.
pub const NOT_YET_COMPATIBLE: i32 = 2;

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExitStatus {
	pub status_code: i32,
	pub at_block: u32,
}

#[link(name = "c")]
extern "C" {
	fn malloc(size: usize) -> *mut c_void;
	fn free(ptr: *mut c_void);
}

#[repr(C)]
pub struct CStrArray {
	// Null pointer if the array isn't initialised.
	c_args: *mut *mut c_char,
	n_args: usize,
}

impl Clone for CStrArray {
	fn clone(&self) -> Self {
		let strings = self.to_rust_strings();
		CStrArray::from_rust_strings(&strings).unwrap()
	}
}

fn malloc_size<T: Sized>(number_of_ts: usize) -> *mut T {
	let alloc = unsafe { malloc(size_of::<T>() * number_of_ts) };

	if alloc.is_null() {
		panic!(
			"Failed to allocate memory of type {} and length {number_of_ts}",
			std::any::type_name::<T>()
		);
	}
	alloc as *mut T
}

impl CStrArray {
	pub fn from_rust_strings(string_args: &[String]) -> anyhow::Result<Self> {
		let mut c_str_array = Self { c_args: std::ptr::null_mut(), n_args: 0 };
		if string_args.is_empty() {
			return Ok(c_str_array);
		}
		c_str_array.c_args = malloc_size::<*mut c_char>(string_args.len());

		for (i, rust_string_arg) in string_args.iter().enumerate() {
			let c_string = CString::new(rust_string_arg.as_str())?;
			let len = c_string.to_bytes_with_nul().len();

			let c_string_ptr = malloc_size::<c_char>(len);

			unsafe {
				std::ptr::copy_nonoverlapping(c_string.as_ptr(), c_string_ptr, len);
				*c_str_array.c_args.add(i) = c_string_ptr;
			}
			c_str_array.n_args = i + 1;
		}
		Ok(c_str_array)
	}

	pub fn to_rust_strings(&self) -> Vec<String> {
		(0..self.n_args)
			.map(|i| {
				let c_str = unsafe { std::ffi::CStr::from_ptr(*self.c_args.add(i)) };
				c_str
					.to_str()
					.expect("We can only get a CStrArray from parsing valid utf8")
					.to_string()
			})
			.collect()
	}
}

impl Drop for CStrArray {
	fn drop(&mut self) {
		if !self.c_args.is_null() {
			unsafe {
				for i in 0..self.n_args {
					let c_string_ptr = *self.c_args.add(i);
					free(c_string_ptr as *mut c_void)
				}
				free(self.c_args as *mut c_void)
			}
		}
	}
}

#[test]
fn test_c_str_array_no_args() {
	let c_args = CStrArray::from_rust_strings(&[]).unwrap();
	assert!(c_args.to_rust_strings().is_empty());
}

#[test]
fn test_c_str_array_with_args() {
	let args = vec!["arg1".to_string(), "arg2".to_string()];

	let c_args = CStrArray::from_rust_strings(&args).unwrap();
	// check the Clone/drop implementations
	{
		let c_args_2 = c_args.clone();
		drop(c_args_2);
	}

	assert_eq!(c_args.to_rust_strings(), args);
}

#[cfg(test)]
mod filter_args_tests {
	use super::*;

	// Stand-ins for the real INCOMPATIBLE_WITH_OLD list, which changes with every release.
	const INCOMPATIBLE: &[IncompatibleArg] = &[
		IncompatibleArg { name: "--incompatible.option", takes_value: true },
		IncompatibleArg { name: "--incompatible.flag", takes_value: false },
	];

	fn filter(args: &[&str]) -> Vec<String> {
		filter_incompatible_args(args.iter().map(|arg| arg.to_string()).collect(), INCOMPATIBLE)
	}

	#[test]
	fn compatible_args_are_preserved() {
		assert_eq!(
			filter(&["chainflip-engine", "--compatible.option=value", "--compatible.flag"]),
			vec!["chainflip-engine", "--compatible.option=value", "--compatible.flag"],
		);
	}

	#[test]
	fn inline_value_is_dropped_with_its_option() {
		assert_eq!(
			filter(&["chainflip-engine", "--incompatible.option=value", "--compatible.flag"]),
			vec!["chainflip-engine", "--compatible.flag"],
		);
	}

	#[test]
	fn split_value_is_dropped_with_its_option() {
		assert_eq!(
			filter(&["chainflip-engine", "--incompatible.option", "value", "--compatible.flag"]),
			vec!["chainflip-engine", "--compatible.flag"],
		);
	}

	#[test]
	fn valueless_flag_does_not_consume_the_next_arg() {
		assert_eq!(
			filter(&["chainflip-engine", "--incompatible.flag", "--compatible.option", "value"]),
			vec!["chainflip-engine", "--compatible.option", "value"],
		);
	}
}
