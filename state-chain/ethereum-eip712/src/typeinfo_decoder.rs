//! Direct SCALE decoder using TypeInfo without building a PortableRegistry.
//!
//! This module provides a decoder that reads SCALE-encoded bytes and produces
//! a `scale_value::Value` by traversing the static `TypeInfo` directly, avoiding
//! the expensive registry construction.

use codec::{Compact, Decode};
use scale_info::{prelude::string::ToString, Type, TypeDef, TypeDefPrimitive, TypeInfo};
use scale_value::{Composite, Value};
use sp_std::{vec, vec::Vec};

/// Error type for decoding failures.
#[derive(Debug)]
pub enum DecodeError {
	/// Not enough bytes in input
	Eof,
	/// Codec error during decoding
	CodecError(codec::Error),
	/// Unsupported type encountered
	UnsupportedType(&'static str),
	/// Invalid variant index
	InvalidVariantIndex(u8),
}

impl From<codec::Error> for DecodeError {
	fn from(e: codec::Error) -> Self {
		DecodeError::CodecError(e)
	}
}

/// Decode SCALE-encoded bytes into a `Value` using the provided `TypeInfo`.
///
/// This function traverses the type structure via `TypeInfo` and decodes
/// the corresponding bytes, avoiding the need to build a `PortableRegistry`.
pub fn decode_with_type_info<T: TypeInfo>(input: &mut &[u8]) -> Result<Value, DecodeError> {
	decode_type(input, &T::type_info())
}

/// Internal recursive decoder that works with `Type` directly.
fn decode_type(input: &mut &[u8], ty: &Type) -> Result<Value, DecodeError> {
	match &ty.type_def {
		TypeDef::Composite(composite) => {
			let fields = &composite.fields;

			if fields.is_empty() {
				// Unit struct
				return Ok(Value::unnamed_composite(vec![]));
			}

			// Check if fields are named or unnamed
			let is_named = fields.first().map(|f| f.name.is_some()).unwrap_or(false);

			if is_named {
				let mut named_fields = Vec::with_capacity(fields.len());
				for field in fields {
					let field_name = field.name.as_ref().unwrap().to_string();
					let field_type = field.ty.type_info();
					let field_value = decode_type(input, &field_type)?;
					named_fields.push((field_name, field_value));
				}
				Ok(Value::named_composite(named_fields))
			} else {
				let mut unnamed_fields = Vec::with_capacity(fields.len());
				for field in fields {
					let field_type = field.ty.type_info();
					let field_value = decode_type(input, &field_type)?;
					unnamed_fields.push(field_value);
				}
				Ok(Value::unnamed_composite(unnamed_fields))
			}
		},

		TypeDef::Variant(variant) => {
			// Read variant index
			let index = u8::decode(input)?;

			let selected_variant = variant
				.variants
				.iter()
				.find(|v| v.index == index)
				.ok_or(DecodeError::InvalidVariantIndex(index))?;

			let variant_name = selected_variant.name.to_string();

			if selected_variant.fields.is_empty() {
				// Unit variant
				Ok(Value::variant(variant_name, Composite::Unnamed(vec![])))
			} else {
				let is_named =
					selected_variant.fields.first().map(|f| f.name.is_some()).unwrap_or(false);

				if is_named {
					let mut named_fields = Vec::with_capacity(selected_variant.fields.len());
					for field in &selected_variant.fields {
						let field_name = field.name.as_ref().unwrap().to_string();
						let field_type = field.ty.type_info();
						let field_value = decode_type(input, &field_type)?;
						named_fields.push((field_name, field_value));
					}
					Ok(Value::variant(variant_name, Composite::Named(named_fields)))
				} else {
					let mut unnamed_fields = Vec::with_capacity(selected_variant.fields.len());
					for field in &selected_variant.fields {
						let field_type = field.ty.type_info();
						let field_value = decode_type(input, &field_type)?;
						unnamed_fields.push(field_value);
					}
					Ok(Value::variant(variant_name, Composite::Unnamed(unnamed_fields)))
				}
			}
		},

		TypeDef::Sequence(seq) => {
			// Sequences are prefixed with compact-encoded length
			let len = Compact::<u32>::decode(input)?.0 as usize;
			let elem_type = seq.type_param.type_info();

			let mut elements = Vec::with_capacity(len);
			for _ in 0..len {
				elements.push(decode_type(input, &elem_type)?);
			}
			Ok(Value::unnamed_composite(elements))
		},

		TypeDef::Array(arr) => {
			let len = arr.len as usize;
			let elem_type = arr.type_param.type_info();

			let mut elements = Vec::with_capacity(len);
			for _ in 0..len {
				elements.push(decode_type(input, &elem_type)?);
			}
			Ok(Value::unnamed_composite(elements))
		},

		TypeDef::Tuple(tuple) => {
			if tuple.fields.is_empty() {
				// Unit type ()
				return Ok(Value::unnamed_composite(vec![]));
			}

			let mut elements = Vec::with_capacity(tuple.fields.len());
			for field_ty in &tuple.fields {
				let field_type = field_ty.type_info();
				elements.push(decode_type(input, &field_type)?);
			}
			Ok(Value::unnamed_composite(elements))
		},

		TypeDef::Primitive(prim) => decode_primitive(input, prim),

		TypeDef::Compact(compact) => {
			// Compact encoding wraps another type
			let inner_type = compact.type_param.type_info();
			decode_compact(input, &inner_type)
		},

		TypeDef::BitSequence(_) => Err(DecodeError::UnsupportedType("BitSequence")),
	}
}

/// Decode a primitive SCALE type.
fn decode_primitive(input: &mut &[u8], prim: &TypeDefPrimitive) -> Result<Value, DecodeError> {
	let value = match prim {
		TypeDefPrimitive::Bool => {
			let b = u8::decode(input)?;
			Value::bool(b != 0)
		},
		TypeDefPrimitive::Char => {
			// SCALE encodes char as u32
			let c = u32::decode(input)?;
			Value::char(
				char::from_u32(c)
					.ok_or(DecodeError::UnsupportedType("invalid char scalar value"))?,
			)
		},
		TypeDefPrimitive::Str => {
			// Strings are compact-length prefixed bytes
			let len = Compact::<u32>::decode(input)?.0 as usize;
			if input.len() < len {
				return Err(DecodeError::Eof);
			}
			let bytes = &input[..len];
			*input = &input[len..];
			let s = core::str::from_utf8(bytes)
				.map_err(|_| DecodeError::UnsupportedType("invalid utf8"))?;
			Value::string(s.to_string())
		},
		TypeDefPrimitive::U8 => {
			let n = u8::decode(input)?;
			Value::u128(n as u128)
		},
		TypeDefPrimitive::U16 => {
			let n = u16::decode(input)?;
			Value::u128(n as u128)
		},
		TypeDefPrimitive::U32 => {
			let n = u32::decode(input)?;
			Value::u128(n as u128)
		},
		TypeDefPrimitive::U64 => {
			let n = u64::decode(input)?;
			Value::u128(n as u128)
		},
		TypeDefPrimitive::U128 => {
			let n = u128::decode(input)?;
			Value::u128(n)
		},
		TypeDefPrimitive::U256 => {
			// U256 is encoded as 4 u64s in little-endian order
			let mut arr = [0u64; 4];
			for item in &mut arr {
				*item = u64::decode(input)?;
			}
			// Store as unnamed composite of u64s (matching scale_value behavior)
			Value::unnamed_composite(arr.iter().map(|&n| Value::u128(n as u128)))
		},
		TypeDefPrimitive::I8 => {
			let n = i8::decode(input)?;
			Value::i128(n as i128)
		},
		TypeDefPrimitive::I16 => {
			let n = i16::decode(input)?;
			Value::i128(n as i128)
		},
		TypeDefPrimitive::I32 => {
			let n = i32::decode(input)?;
			Value::i128(n as i128)
		},
		TypeDefPrimitive::I64 => {
			let n = i64::decode(input)?;
			Value::i128(n as i128)
		},
		TypeDefPrimitive::I128 => {
			let n = i128::decode(input)?;
			Value::i128(n)
		},
		TypeDefPrimitive::I256 => {
			// I256 similar to U256
			let mut arr = [0u64; 4];
			for item in &mut arr {
				*item = u64::decode(input)?;
			}
			Value::unnamed_composite(arr.iter().map(|&n| Value::u128(n as u128)))
		},
	};
	Ok(value)
}

/// Decode a compact-encoded value based on the inner type.
fn decode_compact(input: &mut &[u8], inner_type: &Type) -> Result<Value, DecodeError> {
	// Compact encoding is used for unsigned integers
	match &inner_type.type_def {
		TypeDef::Primitive(prim) => match prim {
			TypeDefPrimitive::U8 => {
				let n = Compact::<u8>::decode(input)?.0;
				Ok(Value::u128(n as u128))
			},
			TypeDefPrimitive::U16 => {
				let n = Compact::<u16>::decode(input)?.0;
				Ok(Value::u128(n as u128))
			},
			TypeDefPrimitive::U32 => {
				let n = Compact::<u32>::decode(input)?.0;
				Ok(Value::u128(n as u128))
			},
			TypeDefPrimitive::U64 => {
				let n = Compact::<u64>::decode(input)?.0;
				Ok(Value::u128(n as u128))
			},
			TypeDefPrimitive::U128 => {
				let n = Compact::<u128>::decode(input)?.0;
				Ok(Value::u128(n))
			},
			_ => Err(DecodeError::UnsupportedType("compact non-unsigned")),
		},
		// Compact can also wrap a composite with a single field (e.g., BlockNumber)
		TypeDef::Composite(composite) if composite.fields.len() == 1 => {
			let inner = composite.fields[0].ty.type_info();
			decode_compact(input, &inner)
		},
		_ => Err(DecodeError::UnsupportedType("compact complex type")),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use scale_info::{prelude::string::String, MetaType, Registry, TypeInfo};

	/// Helper function that decodes using both the new direct TypeInfo method
	/// and the traditional PortableRegistry method, asserting that both produce
	/// identical results.
	fn decode_and_verify<T: TypeInfo + Encode + 'static>(value: &T) -> Value {
		let encoded = value.encode();

		// Decode using the new TypeInfo-based decoder
		let decoded_typeinfo =
			decode_with_type_info::<T>(&mut &encoded[..]).expect("TypeInfo decode failed");

		// Decode using traditional PortableRegistry method
		let mut registry = Registry::new();
		let type_id = registry.register_type(&MetaType::new::<T>());
		let portable_registry: scale_info::PortableRegistry = registry.into();
		let decoded_registry =
			scale_value::scale::decode_as_type(&mut &encoded[..], type_id.id, &portable_registry)
				.expect("Registry decode failed")
				.remove_context();

		// Assert both methods produce identical results
		assert_eq!(
			decoded_typeinfo, decoded_registry,
			"TypeInfo and Registry decoders produced different results"
		);

		decoded_typeinfo
	}

	#[derive(TypeInfo, Encode)]
	struct SimpleStruct {
		a: u32,
		b: String,
	}

	#[derive(TypeInfo, Encode)]
	enum SimpleEnum {
		Variant1,
		Variant2(u64),
		Variant3 { x: u32, y: u32 },
	}

	#[test]
	fn test_decode_simple_struct() {
		let value = SimpleStruct { a: 42, b: "hello".to_string() };
		let _ = decode_and_verify(&value);
	}

	#[test]
	fn test_decode_enum_unit() {
		let value = SimpleEnum::Variant1;
		let _ = decode_and_verify(&value);
	}

	#[test]
	fn test_decode_enum_tuple() {
		let value = SimpleEnum::Variant2(123);
		let _ = decode_and_verify(&value);
	}

	#[test]
	fn test_decode_enum_struct() {
		let value = SimpleEnum::Variant3 { x: 10, y: 20 };
		let _ = decode_and_verify(&value);
	}

	#[test]
	fn test_decode_vec() {
		let value: Vec<u32> = vec![1, 2, 3, 4, 5];
		let _ = decode_and_verify(&value);
	}
}
