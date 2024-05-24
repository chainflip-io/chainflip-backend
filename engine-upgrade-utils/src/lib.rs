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
pub const OLD_VERSION: &str = "1.4.0";
pub const NEW_VERSION: &str = "1.5.0";

pub const ENGINE_LIB_PREFIX: &str = "chainflip_engine_v";
pub const ENGINE_ENTRYPOINT_PREFIX: &str = "cfe_entrypoint_v";

// Sometimes we need to remove arguments that are valid for the new version but not for the old
// version.
// The args that are required for 1.4 but *not* 1.3 are:
// #[derive(Parser, Debug, Clone, Default)]
// pub struct ArbOptions {
// 	#[clap(long = "arb.rpc.ws_endpoint")]
// 	pub arb_ws_endpoint: Option<String>,
// 	#[clap(long = "arb.rpc.http_endpoint")]
// 	pub arb_http_endpoint: Option<String>,
// 	#[clap(long = "arb.private_key_file")]
// 	pub arb_private_key_file: Option<PathBuf>,
// }
pub fn args_compatible_with_old(args: Vec<String>) -> Vec<String> {
	args.into_iter().filter(|arg| !arg.starts_with("--arb.")).collect()
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
