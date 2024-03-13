pub use libc::c_char;
use std::{ffi::CString, mem::size_of};

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
	fn malloc(size: libc::size_t) -> *mut libc::c_void;
	fn free(ptr: *mut libc::c_void);
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn rust_string_args(args: *mut *mut c_char, n_args: u16) -> Vec<String> {
	let mut str_args = Vec::new();
	for i in 0..n_args {
		let c_str = unsafe { std::ffi::CStr::from_ptr(*args.add(i.into())) };
		let str_slice = c_str.to_str().unwrap().to_string();
		str_args.push(str_slice);
	}
	str_args
}

pub fn string_args_to_c_args(str_args: Vec<String>) -> (*mut *mut c_char, u16) {
	let n_args = str_args.len() as u16;

	let ptrs_size = str_args.len() * size_of::<*mut c_char>();
	let c_array_ptr = unsafe { malloc(ptrs_size as libc::size_t) as *mut *mut c_char };

	if c_array_ptr.is_null() {
		panic!("Failed to allocate memory for the Command Line Args array");
	}

	for (i, rust_string_arg) in str_args.iter().enumerate() {
		let c_string = CString::new(rust_string_arg.as_str()).unwrap();
		let len = c_string.to_bytes_with_nul().len();

		let c_string_ptr = unsafe { malloc(len * size_of::<c_char>()) as *mut c_char };
		if c_string_ptr.is_null() {
			panic!("Failed to allocate memory for the Command Line Arg");
			// clean up the previous allocations?
		}

		unsafe {
			std::ptr::copy_nonoverlapping(c_string.as_ptr(), c_string_ptr, len);
			*c_array_ptr.add(i) = c_string_ptr;
		}
	}
	(c_array_ptr, n_args)
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn free_c_args(c_args: *mut *mut c_char, len: u16) {
	unsafe {
		for i in 0..len {
			let c_string_ptr = *c_args.add(i.into());
			free(c_string_ptr as *mut libc::c_void)
		}
		free(c_args as *mut libc::c_void)
	}
}

#[test]
fn test_rust_string_args() {
	let args = vec!["arg1".to_string(), "arg2".to_string()];
	let c_args = string_args_to_c_args(args.clone());
	let rust_args = rust_string_args(c_args.0, c_args.1);
	assert_eq!(args, rust_args);
	free_c_args(c_args.0, c_args.1);

	assert_eq!(rust_string_args(std::ptr::null_mut(), 0), Vec::<String>::new());
}
