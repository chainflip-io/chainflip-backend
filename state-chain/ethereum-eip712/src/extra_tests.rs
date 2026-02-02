use crate::{
	build_eip712_data::{build_eip712_typed_data, to_ethers_typed_data},
	eip712::{Eip712, TypedData},
	*,
};

use core::marker::PhantomData;
use ethers_core::types::{H256, U128, U256};
use scale_info::prelude::string::String;
use serde::Deserialize;
use std::{
	io::Write,
	process::{Command, Stdio},
};

pub mod test_types {
	use super::*;
	#[derive(TypeInfo, Encode)]
	pub enum TestEnum<T, S> {
		A(T),
		B(S),
		C(Vec<u128>),
		D { dd: u16 },
	}
	#[derive(TypeInfo, Encode)]
	pub struct TestEmpty;
	#[derive(TypeInfo, Encode)]
	pub struct TestEmptyNested(pub TestEmpty);

	#[derive(TypeInfo, Encode)]
	pub struct TestCompact<T, S> {
		#[codec(compact)]
		pub c: u128,
		pub a: T,
		pub b: S,
	}

	#[derive(TypeInfo, Encode)]
	pub struct TestEmptyGeneric<T>(pub PhantomData<T>);

	#[derive(TypeInfo, Encode)]
	pub enum TestEmptyEnum {
		Aaaaa,
		Bbbbb,
	}

	#[derive(TypeInfo, Encode)]
	pub struct TestWrappedEmptyEnum(pub TestEmptyEnum);

	#[derive(TypeInfo, Encode)]
	pub struct Abs(pub (u8, u128));
	#[derive(Encode, TypeInfo)]
	pub struct Mail {
		pub from: String,
		pub to: String,
		pub message: String,
	}

	#[derive(Encode, TypeInfo)]
	pub struct TestVec<T>(pub Vec<T>);
	#[derive(Encode, TypeInfo)]
	pub struct TestArray(pub [u16; 10]);

	#[derive(Encode, TypeInfo)]
	pub struct TetsPrimitiveTypeU256(pub U256);

	#[derive(Encode, TypeInfo)]
	pub struct TetsPrimitiveTypeU128(pub U128);

	#[derive(Encode, TypeInfo)]
	pub struct TetsPrimitiveTypeH256(pub H256);

	#[derive(Encode, TypeInfo)]
	pub struct TestI128(pub i128);

	pub mod test_complex_type_with_vecs_and_enums {
		use super::*;
		// Define complex types with vectors and enums
		#[derive(TypeInfo, Clone, Encode)]
		pub struct TypeWithVector {
			pub items: Vec<u32>,
			pub description: String,
		}

		#[derive(TypeInfo, Clone, Encode)]
		pub enum StatusEnum {
			Active,
			Pending { reason: String },
			Completed { count: u64, timestamp: u128 },
		}

		#[derive(TypeInfo, Clone, Encode)]
		pub struct TypeWithEnum {
			pub status: StatusEnum,
			pub id: u64,
		}

		#[derive(TypeInfo, Clone, Encode)]
		pub enum Priority {
			Low,
			Medium,
			High,
		}

		#[derive(TypeInfo, Clone, Encode)]
		pub struct TypeWithBoth {
			pub tags: Vec<String>,
			pub priority: Priority,
			pub nested_items: Vec<u16>,
		}

		#[derive(TypeInfo, Clone, Encode)]
		pub struct ComplexRoot {
			pub field_with_vector: TypeWithVector,
			pub field_with_vector_2: TypeWithVector,
			pub field_with_enum: TypeWithEnum,
			pub field_with_both: TypeWithBoth,
			pub field_with_enum_2: TypeWithEnum,
			pub field_with_enum_3: TypeWithEnum,
		}
	}

	pub mod test_vec_of_enum {
		use super::*;
		// Simple enum type
		#[derive(TypeInfo, Clone, Encode)]
		pub enum Color {
			Red,
			Green,
			Blue { intensity: u8 },
		}

		// Struct with Vec of enum
		#[derive(TypeInfo, Clone, Encode)]
		pub struct InnerStruct {
			pub colors: Vec<Color>,
		}

		// Root struct with one field
		#[derive(TypeInfo, Clone, Encode)]
		pub struct SimpleRoot {
			pub inner: InnerStruct,
		}
	}
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum HasherResponse {
	Success {
		library: String,
		#[serde(rename = "signingHash")]
		signing_hash: H256,
	},
	Error {
		error: String,
	},
}

const LIBRARIES: &[&str] = &["ethers", "viem", "eth-sig-util"];
const TS_SIGNER_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/ts-signer");

#[derive(Clone)]
enum JsRuntime {
	/// Pre-compiled standalone binary (fastest)
	CompiledBinary(String),
	/// Run via bun
	Bun,
	/// Run via npx tsx (Node.js)
	Node,
}

fn get_compiled_binary_path() -> Option<String> {
	#[cfg(target_os = "linux")]
	let binary_name = "eip712-signer-linux-x64";
	#[cfg(target_os = "macos")]
	let binary_name = "eip712-signer-darwin-arm64";
	#[cfg(not(any(target_os = "linux", target_os = "macos")))]
	let binary_name = "";

	let path = std::path::Path::new(TS_SIGNER_DIR).join("dist").join(binary_name);

	if path.exists() {
		path.to_str().map(String::from)
	} else {
		None
	}
}

fn detect_js_runtime() -> Option<JsRuntime> {
	// First, check for pre-compiled binary (fastest)
	if let Some(binary_path) = get_compiled_binary_path() {
		return Some(JsRuntime::CompiledBinary(binary_path));
	}

	// Check if node_modules exists (dependencies installed)
	let node_modules = std::path::Path::new(TS_SIGNER_DIR).join("node_modules");
	if !node_modules.exists() {
		return None;
	}

	if Command::new("bun").arg("--version").output().is_ok() {
		Some(JsRuntime::Bun)
	} else if Command::new("npx").arg("--version").output().is_ok() {
		Some(JsRuntime::Node)
	} else {
		None
	}
}

fn hash_with_ts_lib(
	typed_data: &TypedData,
	library: &str,
	js_runtime: &JsRuntime,
) -> HasherResponse {
	let ethers_typed_data =
		to_ethers_typed_data(typed_data.clone()).expect("Failed to convert to ethers typed data");
	let typed_data_json =
		serde_json::to_string(&ethers_typed_data).expect("Failed to serialize typed data to JSON");

	let mut child = match js_runtime {
		JsRuntime::CompiledBinary(path) => Command::new(path)
			.args(["--library", library])
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.expect("Failed to spawn compiled binary"),
		JsRuntime::Bun => Command::new("bun")
			.args(["run", "src/main.ts", "--library", library])
			.current_dir(TS_SIGNER_DIR)
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.expect("Failed to spawn bun process"),
		JsRuntime::Node => Command::new("npx")
			.args(["tsx", "src/main.ts", "--library", library])
			.current_dir(TS_SIGNER_DIR)
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.expect("Failed to spawn npx tsx process"),
	};

	child
		.stdin
		.as_mut()
		.expect("Failed to open stdin")
		.write_all(typed_data_json.as_bytes())
		.expect("Failed to write to stdin");

	let output = child.wait_with_output().expect("Failed to wait for eip712-hasher");

	if !output.status.success() {
		let stderr =
			std::string::String::from_utf8(output.stderr).expect("Invalid UTF-8 in stderr");
		panic!("eip712-hasher failed with status {}: {}", output.status, stderr);
	}

	serde_json::from_slice(&output.stdout).expect("Failed to parse eip712-hasher output")
}

macro_rules! eip712_test {
	($name:ident, $expr:expr) => {
		#[test]
		fn $name() {
			let Some(runtime) = detect_js_runtime() else {
				panic!("Skipping test: neither bun nor node/npx is installed");
			};

			let test_value = $expr;
			let typed_data =
				build_eip712_typed_data(test_value, "Chainflip-Mainnet".to_string(), 1)
					.expect("Failed to build EIP-712 typed data");

			let rust_encoded = typed_data.encode_eip712().expect("Failed to encode typed data");
			let rust_hash = H256(keccak256(&rust_encoded));

			for library in LIBRARIES {
				let response = hash_with_ts_lib(&typed_data, library, &runtime);

				match response {
					HasherResponse::Success { library: lib, signing_hash } => {
						assert_eq!(
							rust_hash, signing_hash,
							"Hash mismatch with {} library.\nRust: 0x{}\nJS:   0x{}",
							lib, rust_hash, signing_hash,
						);
					},
					HasherResponse::Error { error } => {
						panic!("eip712-hasher ({}) returned error: {}", library, error);
					},
				}
			}
		}
	};
}

eip712_test!(test_i128_small_pos, test_types::TestI128(12345));
eip712_test!(test_i128_small_neg, test_types::TestI128(-12345));
eip712_test!(test_enum_a_i32, test_types::TestEnum::<i32, ()>::A(-12345));
eip712_test!(test_enum_a_u32, test_types::TestEnum::<u32, u8>::A(5));
eip712_test!(test_enum_b_u8, test_types::TestEnum::<u32, u8>::B(6));
eip712_test!(test_enum_c_vec, test_types::TestEnum::<u32, u8>::C(vec![7]));
eip712_test!(test_enum_d_struct, test_types::TestEnum::<u32, u8>::D { dd: 8 });
eip712_test!(
	test_wrapped_empty_enum_a,
	test_types::TestWrappedEmptyEnum(test_types::TestEmptyEnum::Aaaaa)
);
eip712_test!(
	test_wrapped_empty_enum_b,
	test_types::TestWrappedEmptyEnum(test_types::TestEmptyEnum::Bbbbb)
);
eip712_test!(test_compact, test_types::TestCompact { a: 5u8, b: 6u32, c: 7 });
eip712_test!(test_empty_nested, test_types::TestEmptyNested(test_types::TestEmpty));
eip712_test!(test_abs_tuple, test_types::Abs((8, 9)));
eip712_test!(test_vec_u8_empty, test_types::TestVec::<u8>(vec![]));
eip712_test!(test_vec_u128_empty, test_types::TestVec::<u128>(vec![]));
eip712_test!(test_vec_u128, test_types::TestVec(vec![5u128, 6u128]));
eip712_test!(test_vec_u8, test_types::TestVec(vec![5u8, 6u8]));
eip712_test!(test_array_u16, test_types::TestArray([5u16; 10]));
eip712_test!(test_primitive_u256, test_types::TetsPrimitiveTypeU256(U256([1, 2, 3, 4])));
eip712_test!(test_primitive_u128, test_types::TetsPrimitiveTypeU128(U128([1, 2])));
eip712_test!(test_primitive_h256, test_types::TetsPrimitiveTypeH256(H256([5u8; 32])));
eip712_test!(test_tuple_with_abs, (12u128, test_types::Abs((8, 10))));

eip712_test!(
	test_enum_nested_enum,
	test_types::TestEnum::<test_types::TestEmptyEnum, u8>::A(test_types::TestEmptyEnum::Aaaaa)
);
eip712_test!(
	test_enum_with_struct_a,
	test_types::TestEnum::<test_types::Abs, test_types::TestEmpty>::A(test_types::Abs((1, 2)))
);
eip712_test!(
	test_enum_with_struct_b,
	test_types::TestEnum::<test_types::Abs, test_types::TestEmpty>::B(test_types::TestEmpty)
);
eip712_test!(
	test_vec_of_structs,
	test_types::TestVec(vec![test_types::Abs((1, 2)), test_types::Abs((3, 4))])
);
eip712_test!(
	test_compact_with_structs,
	test_types::TestCompact { a: test_types::TestEmpty, b: test_types::Abs((1, 2)), c: 100 }
);
eip712_test!(
	test_mail,
	test_types::Mail {
		from: "alice@example.com".to_string(),
		to: "bob@example.com".to_string(),
		message: "Hello!".to_string()
	}
);
eip712_test!(
	test_mail_empty_strings,
	test_types::Mail { from: "".to_string(), to: "".to_string(), message: "".to_string() }
);
eip712_test!(
	test_vec_nested_structs,
	test_types::TestVec(vec![test_types::TestEmptyNested(test_types::TestEmpty)])
);
eip712_test!(test_primitive_u256_zero, test_types::TetsPrimitiveTypeU256(U256::zero()));
eip712_test!(test_primitive_u128_zero, test_types::TetsPrimitiveTypeU128(U128::zero()));
eip712_test!(test_primitive_h256_zero, test_types::TetsPrimitiveTypeH256(H256::zero()));

// Large integer tests (beyond JavaScript's MAX_SAFE_INTEGER = 2^53 - 1 = 9007199254740991)
const JS_MAX_SAFE_INTEGER: u128 = 2u128.pow(53) - 1;

eip712_test!(test_u64_max, test_types::TestVec(vec![u64::MAX]));
eip712_test!(test_u128_large, test_types::TestVec(vec![u128::MAX]));
eip712_test!(test_u128_beyond_js_safe, test_types::TestVec(vec![JS_MAX_SAFE_INTEGER + 1]));
eip712_test!(test_compact_large_value, test_types::TestCompact { a: 1u8, b: 2u32, c: u128::MAX });
eip712_test!(test_abs_large_u128, test_types::Abs((255, u128::MAX)));
eip712_test!(test_primitive_u256_large, test_types::TetsPrimitiveTypeU256(U256::MAX));
eip712_test!(test_primitive_u128_max, test_types::TetsPrimitiveTypeU128(U128::MAX));
eip712_test!(
	test_vec_mixed_large_u128,
	test_types::TestVec(vec![JS_MAX_SAFE_INTEGER, JS_MAX_SAFE_INTEGER + 1, u128::MAX])
);
eip712_test!(test_i128_min, test_types::TestI128(i128::MIN));
eip712_test!(test_i128_max, test_types::TestI128(i128::MAX));
eip712_test!(test_i128_negative_large, test_types::TestI128(-(JS_MAX_SAFE_INTEGER as i128 + 1)));
eip712_test!(test_enum_negative_large, test_types::TestEnum::<i32, ()>::A(i32::MIN));
eip712_test!(
	test_complex_root,
	test_types::test_complex_type_with_vecs_and_enums::ComplexRoot {
		field_with_vector: test_types::test_complex_type_with_vecs_and_enums::TypeWithVector {
			items: vec![1, 2, 3, 4, 5],
			description: "Test description".to_string(),
		},
		field_with_vector_2: test_types::test_complex_type_with_vecs_and_enums::TypeWithVector {
			items: vec![],
			description: "Empty items".to_string(),
		},
		field_with_enum: test_types::test_complex_type_with_vecs_and_enums::TypeWithEnum {
			status: test_types::test_complex_type_with_vecs_and_enums::StatusEnum::Pending {
				reason: "Waiting for approval".to_string()
			},
			id: 42,
		},
		field_with_both: test_types::test_complex_type_with_vecs_and_enums::TypeWithBoth {
			tags: vec!["tag1".to_string(), "tag2".to_string(), "tag3".to_string()],
			priority: test_types::test_complex_type_with_vecs_and_enums::Priority::High,
			nested_items: vec![100, 200, 300],
		},
		field_with_enum_2: test_types::test_complex_type_with_vecs_and_enums::TypeWithEnum {
			status: test_types::test_complex_type_with_vecs_and_enums::StatusEnum::Active,
			id: 50
		},
		field_with_enum_3: test_types::test_complex_type_with_vecs_and_enums::TypeWithEnum {
			status: test_types::test_complex_type_with_vecs_and_enums::StatusEnum::Completed {
				count: 5,
				timestamp: 6
			},
			id: 60,
		},
	}
);
eip712_test!(
	test_simple_root,
	test_types::test_vec_of_enum::SimpleRoot {
		inner: test_types::test_vec_of_enum::InnerStruct {
			colors: vec![
				test_types::test_vec_of_enum::Color::Red,
				test_types::test_vec_of_enum::Color::Blue { intensity: 128 },
				test_types::test_vec_of_enum::Color::Green
			],
		},
	}
);
