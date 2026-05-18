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
pub const OLD_VERSION: &str = "2.1.18";
pub const NEW_VERSION: &str = "2.2.0";

pub const ENGINE_LIB_PREFIX: &str = "chainflip_engine_v";
pub const ENGINE_ENTRYPOINT_PREFIX: &str = "cfe_entrypoint_v";

// Sometimes we need to adapt arguments between the new and old versions while both CFEs can be run
// by the upgrade runner.
pub fn args_compatible_with_old(args: Vec<String>) -> Vec<String> {
	let mut compatible_args = args;

	// Old CFE 2.1.x still requires and validates EVM websocket endpoints in its settings model,
	// and constructs websocket subscription clients from them. The active ETH/ARB witnessing path
	// uses HTTP polling, though, so these synthesized websocket URLs only satisfy old config
	// parsing during the upgrade fallback path.
	add_ws_endpoint_from_http_endpoint(
		&mut compatible_args,
		"eth.rpc.http_endpoint",
		"eth.rpc.ws_endpoint",
	);
	add_ws_endpoint_from_http_endpoint(
		&mut compatible_args,
		"eth.backup_rpc.http_endpoint",
		"eth.backup_rpc.ws_endpoint",
	);
	add_ws_endpoint_from_http_endpoint(
		&mut compatible_args,
		"arb.rpc.http_endpoint",
		"arb.rpc.ws_endpoint",
	);
	add_ws_endpoint_from_http_endpoint(
		&mut compatible_args,
		"arb.backup_rpc.http_endpoint",
		"arb.backup_rpc.ws_endpoint",
	);

	copy_arg(&mut compatible_args, "hub.rpc.ws_endpoint", "dot.rpc.ws_endpoint");
	copy_arg(&mut compatible_args, "hub.rpc.http_endpoint", "dot.rpc.http_endpoint");
	copy_arg(&mut compatible_args, "hub.backup_rpc.ws_endpoint", "dot.backup_rpc.ws_endpoint");
	copy_arg(&mut compatible_args, "hub.backup_rpc.http_endpoint", "dot.backup_rpc.http_endpoint");

	compatible_args.retain(|arg| !arg.starts_with("--tron."));

	compatible_args
}

fn add_ws_endpoint_from_http_endpoint(args: &mut Vec<String>, http_key: &str, ws_key: &str) {
	if has_arg(args, ws_key) {
		return;
	}

	let Some(http_endpoint) = find_arg_value(args, http_key) else {
		return;
	};

	let ws_endpoint = if let Some(endpoint) = http_endpoint.strip_prefix("https://") {
		format!("wss://{endpoint}")
	} else if let Some(endpoint) = http_endpoint.strip_prefix("http://") {
		format!("ws://{endpoint}")
	} else {
		return;
	};

	args.push(format!("--{ws_key}={ws_endpoint}"));
}

fn copy_arg(args: &mut Vec<String>, source_key: &str, target_key: &str) {
	if has_arg(args, target_key) {
		return;
	}

	if let Some(value) = find_arg_value(args, source_key) {
		args.push(format!("--{target_key}={value}"));
	}
}

fn has_arg(args: &[String], key: &str) -> bool {
	args.iter().any(|arg| {
		arg.strip_prefix("--").is_some_and(|arg_without_prefix| {
			arg_without_prefix == key || arg_without_prefix.starts_with(&format!("{key}="))
		})
	})
}

fn find_arg_value(args: &[String], key: &str) -> Option<String> {
	args.iter()
		.find_map(|arg| arg.strip_prefix(&format!("--{key}=")).map(ToString::to_string))
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

#[test]
fn test_args_compatible_with_old_adds_removed_settings() {
	let args = vec![
		"chainflip-engine".to_string(),
		"--eth.rpc.http_endpoint=https://eth-rpc.example.com/secret".to_string(),
		"--eth.backup_rpc.http_endpoint=http://eth-backup.example.com".to_string(),
		"--arb.rpc.http_endpoint=http://arb-rpc.example.com".to_string(),
		"--arb.backup_rpc.http_endpoint=https://arb-backup.example.com".to_string(),
		"--hub.rpc.ws_endpoint=wss://hub-rpc.example.com/secret".to_string(),
		"--hub.rpc.http_endpoint=https://hub-rpc.example.com/secret".to_string(),
		"--hub.backup_rpc.ws_endpoint=ws://hub-backup.example.com".to_string(),
		"--hub.backup_rpc.http_endpoint=http://hub-backup.example.com".to_string(),
	];

	let compatible_args = args_compatible_with_old(args);

	assert!(compatible_args
		.contains(&"--eth.rpc.ws_endpoint=wss://eth-rpc.example.com/secret".to_string()));
	assert!(compatible_args
		.contains(&"--eth.backup_rpc.ws_endpoint=ws://eth-backup.example.com".to_string()));
	assert!(compatible_args.contains(&"--arb.rpc.ws_endpoint=ws://arb-rpc.example.com".to_string()));
	assert!(compatible_args
		.contains(&"--arb.backup_rpc.ws_endpoint=wss://arb-backup.example.com".to_string()));
	assert!(compatible_args
		.contains(&"--dot.rpc.ws_endpoint=wss://hub-rpc.example.com/secret".to_string()));
	assert!(compatible_args
		.contains(&"--dot.rpc.http_endpoint=https://hub-rpc.example.com/secret".to_string()));
	assert!(compatible_args
		.contains(&"--dot.backup_rpc.ws_endpoint=ws://hub-backup.example.com".to_string()));
	assert!(compatible_args
		.contains(&"--dot.backup_rpc.http_endpoint=http://hub-backup.example.com".to_string()));
}
