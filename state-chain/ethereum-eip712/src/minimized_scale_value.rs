use crate::*;
use codec::{Decode, Encode};

/// A "primitive" value (this includes strings).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Serialize, Deserialize, TypeInfo)]
pub enum MinimizedPrimitive {
	/// A boolean value.
	Bool(bool),
	/// A single ASCII character.
	Char(u8), // Note: `char` doesn't implement `Encode`/`Decode`
	/// A string.
	String(String),
	/// A u128 value.
	U128(u128),
	/// An i128 value.
	I128(i128),
	/// An unsigned 256 bit number (internally represented as a 32 byte array).
	U256([u8; 32]),
	/// A signed 256 bit number (internally represented as a 32 byte array).
	I256([u8; 32]),
}

impl From<scale_value::Primitive> for MinimizedPrimitive {
	fn from(p: scale_value::Primitive) -> Self {
		match p {
			scale_value::Primitive::Bool(b) => MinimizedPrimitive::Bool(b),
			scale_value::Primitive::Char(c) => MinimizedPrimitive::Char(c as u8),
			scale_value::Primitive::String(s) => MinimizedPrimitive::String(s),
			scale_value::Primitive::U128(n) => MinimizedPrimitive::U128(n),
			scale_value::Primitive::U256(bytes) => MinimizedPrimitive::U256(bytes),
			scale_value::Primitive::I128(n) => MinimizedPrimitive::I128(n),
			scale_value::Primitive::I256(bytes) => MinimizedPrimitive::I256(bytes),
		}
	}
}

impl From<MinimizedPrimitive> for scale_value::Primitive {
	fn from(p: MinimizedPrimitive) -> Self {
		match p {
			MinimizedPrimitive::Bool(b) => scale_value::Primitive::Bool(b),
			MinimizedPrimitive::Char(c) => scale_value::Primitive::Char(c as char),
			MinimizedPrimitive::String(s) => scale_value::Primitive::String(s),
			MinimizedPrimitive::U128(n) => scale_value::Primitive::U128(n),
			MinimizedPrimitive::I128(n) => scale_value::Primitive::I128(n),
			MinimizedPrimitive::U256(bytes) => scale_value::Primitive::U256(bytes),
			MinimizedPrimitive::I256(bytes) => scale_value::Primitive::I256(bytes),
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Serialize, Deserialize, TypeInfo)]
pub enum MinimizedScaleValue {
	NamedStruct(Vec<(String, MinimizedScaleValue)>),
	Sequence(Vec<MinimizedScaleValue>),
	Primitive(MinimizedPrimitive),
}

impl TryFrom<Value> for MinimizedScaleValue {
	type Error = &'static str;

	fn try_from(value: Value) -> Result<Self, Self::Error> {
		match value.value {
			ValueDef::Composite(Composite::Named(fs)) => Ok(Self::NamedStruct(
				fs.into_iter()
					.map(|(name, v)| Ok((name, MinimizedScaleValue::try_from(v)?)))
					.collect::<Result<Vec<(String, Self)>, &'static str>>()?,
			)),
			ValueDef::Composite(Composite::Unnamed(fs)) => Ok(Self::Sequence(
				fs.into_iter()
					.map(MinimizedScaleValue::try_from)
					.collect::<Result<Vec<Self>, &'static str>>()?,
			)),
			ValueDef::Variant(_) =>
				Err("scale value with variant cannot be converted to MinimizedScaleValue"),
			ValueDef::Primitive(p) => Ok(Self::Primitive(p.into())),
			ValueDef::BitSequence(_) => Err("BitSequence not supported"),
		}
	}
}

impl MinimizedScaleValue {
	pub fn get_struct_field(&self, field_name: String) -> Result<Self, String> {
		match &self {
			Self::NamedStruct(fs) => fs
				.iter()
				.find(|(name, _)| *name == field_name)
				.ok_or(format!("field with this name not found: {:?}", field_name))
				.map(|(_, v)| (*v).clone()),
			_ => Err("this value is not a struct".to_string()),
		}
	}

	#[allow(clippy::result_unit_err)]
	pub fn extract_hex_bytes(&self) -> Result<Vec<u8>, ()> {
		if let Self::Primitive(MinimizedPrimitive::String(s)) = self.clone() {
			hex::decode(s).map_err(|_| ())
		} else {
			Err(())
		}
	}
}

impl From<MinimizedScaleValue> for Value {
	fn from(value: MinimizedScaleValue) -> Self {
		match value {
			MinimizedScaleValue::NamedStruct(fs) => Value {
				value: ValueDef::Composite(Composite::Named(
					fs.into_iter().map(|(name, v)| (name, Value::from(v))).collect(),
				)),
				context: (),
			},
			MinimizedScaleValue::Sequence(fs) => Value {
				value: ValueDef::Composite(Composite::Unnamed(
					fs.into_iter().map(Value::from).collect(),
				)),
				context: (),
			},
			MinimizedScaleValue::Primitive(p) =>
				Value { value: ValueDef::Primitive(p.into()), context: () },
		}
	}
}
