use std::{
	ffi::{c_void, CString},
	mem::size_of,
};

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

pub struct CStrArray {
	c_args: *mut *mut c_char,
	// Nnoe if the outer array isn't initialized
	n_args: Option<usize>,
}

impl Default for CStrArray {
	fn default() -> Self {
		Self { c_args: std::ptr::null_mut(), n_args: None }
	}
}

impl CStrArray {
	pub fn string_args_to_c_args(&mut self, string_args: Vec<String>) -> anyhow::Result<()> {
		let ptrs_size = string_args.len() * size_of::<*mut c_char>();
		let array_malloc = unsafe { malloc(ptrs_size) };

		if array_malloc.is_null() {
			panic!("Failed to allocate memory for the Command Line Args array");
		}

		let c_array_ptr = array_malloc as *mut *mut c_char;
		self.c_args = c_array_ptr;
		self.n_args = Some(0);

		for (i, rust_string_arg) in string_args.iter().enumerate() {
			let c_string = CString::new(rust_string_arg.as_str())?;
			let len = c_string.to_bytes_with_nul().len();

			let c_string_ptr = unsafe { malloc(len * size_of::<c_char>()) };
			if c_string_ptr.is_null() {
				panic!("Failed to allocate memory for the Command Line Arg");
			}
			let c_string_ptr = c_string_ptr as *mut c_char;

			unsafe {
				std::ptr::copy_nonoverlapping(c_string.as_ptr(), c_string_ptr, len);
				*c_array_ptr.add(i) = c_string_ptr;
			}
			self.n_args = Some(i + 1);
		}
		Ok(())
	}

	pub fn get_args(&mut self) -> (*mut *mut c_char, usize) {
		(self.c_args, self.n_args.unwrap_or_default())
	}
}

impl Drop for CStrArray {
	fn drop(&mut self) {
		if let Some(n_args) = self.n_args {
			unsafe {
				for i in 0..n_args {
					let c_string_ptr = *self.c_args.add(i);
					free(c_string_ptr as *mut c_void)
				}
				free(self.c_args as *mut c_void)
			}
		}
	}
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn rust_string_args(args: *mut *mut c_char, n_args: usize) -> Vec<String> {
	let mut str_args = Vec::new();
	for i in 0..n_args {
		let c_str = unsafe { std::ffi::CStr::from_ptr(*args.add(i)) };
		let str_slice = c_str.to_str().unwrap().to_string();
		str_args.push(str_slice);
	}
	str_args
}

#[test]
fn test_c_str_array_no_args() {
	let mut c_args = CStrArray::default();
	let (c_args, n_args) = c_args.get_args();
	assert_eq!(rust_string_args(c_args, n_args), Vec::<String>::new());
}

#[test]
fn test_c_str_array_with_args() {
	let args = vec!["arg1".to_string(), "arg2".to_string()];

	let mut c_args = CStrArray::default();
	c_args.string_args_to_c_args(args.clone()).unwrap();

	let (c_args, n_args) = c_args.get_args();
	let rust_args = rust_string_args(c_args, n_args);
	assert_eq!(args, rust_args);
}
