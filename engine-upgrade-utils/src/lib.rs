use std::{
	ffi::{c_void, CString},
	mem::size_of,
};

// !!!!!!! These constants are used to check the versions across several crates and build scripts.
// These should be the first things changed when bumping the version, as it will check the
// rest of the places the version needs changing on build using the build scripts in each of the
// relevant crates.
// Should also check that the compatibility function below `args_compatible_with_old` is correct.
pub const OLD_VERSION: &str = "1.3.0";
pub const NEW_VERSION: &str = "1.4.0";

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
#[derive(Clone)]
pub struct CStrArray {
	c_args: *mut *mut c_char,
	// Nnoe if the outer array isn't initialized
	n_args: usize,
}

impl Default for CStrArray {
	fn default() -> Self {
		Self { c_args: std::ptr::null_mut(), n_args: 0 }
	}
}

fn malloc_size<T: Sized>(number_of_ts: usize) -> *mut c_void {
	unsafe { malloc(size_of::<T>() * number_of_ts) }
}

impl CStrArray {
	pub fn string_args_to_c_args(&mut self, string_args: Vec<String>) -> anyhow::Result<()> {
		let array_malloc = malloc_size::<*mut c_char>(string_args.len());

		if array_malloc.is_null() {
			panic!("Failed to allocate memory for the Command Line Args array");
		}

		let c_array_ptr = array_malloc as *mut *mut c_char;
		self.c_args = c_array_ptr;

		for (i, rust_string_arg) in string_args.iter().enumerate() {
			let c_string = CString::new(rust_string_arg.as_str())?;
			let len = c_string.to_bytes_with_nul().len();

			let c_string_ptr = malloc_size::<c_char>(len);
			if c_string_ptr.is_null() {
				panic!("Failed to allocate memory for the Command Line Arg");
			}
			let c_string_ptr = c_string_ptr as *mut c_char;

			unsafe {
				std::ptr::copy_nonoverlapping(c_string.as_ptr(), c_string_ptr, len);
				*c_array_ptr.add(i) = c_string_ptr;
			}
			self.n_args = i + 1;
		}
		Ok(())
	}

	pub fn rust_string_args(&self) -> Vec<String> {
		let mut str_args = Vec::new();
		for i in 0..self.n_args {
			let c_str = unsafe { std::ffi::CStr::from_ptr(*self.c_args.add(i)) };
			let str_slice = c_str.to_str().unwrap().to_string();
			str_args.push(str_slice);
		}
		str_args
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
	let c_args = CStrArray::default();
	assert!(c_args.rust_string_args().is_empty());
}

#[test]
fn test_c_str_array_with_args() {
	let args = vec!["arg1".to_string(), "arg2".to_string()];
	let mut c_args = CStrArray::default();
	c_args.string_args_to_c_args(args.clone()).unwrap();
	assert_eq!(c_args.rust_string_args(), args);
}
