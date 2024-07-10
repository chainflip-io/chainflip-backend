use scale_decode::{
	visitor::{decode_with_visitor, DecodeAsTypeResult, DecodeError},
	IntoVisitor, TypeResolver,
};
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

/// Re-export useful types from transitive dependencies.
pub mod ext {
	pub use scale_decode::DecodeAsType;
	pub use serde_json::Value as JsonValue;
}
pub use scale_decode::Error;

mod test;

// TODO:
// - Consider using the (newly added) composite name() or path() to improve decoding of (for
//   example) account_ids. Or maybe add it as an additional __TYPE_HINT__ field in the JSON output.

/// A wrapper type around a `serde_json::Value` that allows decoding scale encoded data into JSON.
///
/// Byte arrays are decoded as a single hex string (rather than an array of numbers).
/// Numbers up to 32 bits wide are decoded as JSON Numbers, anything 64 bits wide or above is
/// decoded as hex-encoded JSON Strings.
///
/// A heuristic is applied to arrays and sequences to determine if they should be interpreted as
/// byte arrays: If the sequence or array is not empty, *and* if each contained item decodes to a
/// json Number value, *and* each item was encoded as a single byte, then the sequence or array is
/// interpreted as a sequence or array of bytes.
///
/// # Example
///
/// ```
/// use scale_json::{
///     ScaleDecodedToJson,
///     ext::{DecodeAsType, JsonValue},
/// };
/// use scale_info::{PortableRegistry, TypeInfo};
/// use codec::{Encode, Decode};
///
/// fn make_type_resolver<T: TypeInfo + 'static>() -> (u32, PortableRegistry) {
///     let m = scale_info::MetaType::new::<T>();
///     let mut registry = scale_info::Registry::new();
///     let type_id = registry.register_type(&m).id;
///     (type_id, PortableRegistry::from(registry))
/// }
///
/// let (type_id, registry) = make_type_resolver::<Vec<u32>>();
///
/// let scale_encoded = vec![0u32, 1, 2, 3].encode();
///
/// let decoded_json: JsonValue = <ScaleDecodedToJson as DecodeAsType>::decode_as_type(
///    &mut &scale_encoded[..],
///    type_id,
///    &registry,
/// ).unwrap().into();
///
/// assert!(decoded_json.is_array());
/// assert_eq!(decoded_json[2], JsonValue::Number(2.into()));
/// ```
#[derive(Debug, Serialize, Deserialize)]
pub struct ScaleDecodedToJson(serde_json::Value);

impl From<serde_json::Value> for ScaleDecodedToJson {
	fn from(value: serde_json::Value) -> Self {
		Self(value)
	}
}
impl From<ScaleDecodedToJson> for serde_json::Value {
	fn from(value: ScaleDecodedToJson) -> Self {
		value.0
	}
}
impl AsRef<serde_json::Value> for ScaleDecodedToJson {
	fn as_ref(&self) -> &serde_json::Value {
		&self.0
	}
}

// Implementing this is what gives us `DecodeAsType` for `ScaleDecodedToJson`.
impl IntoVisitor for ScaleDecodedToJson {
	type AnyVisitor<T: TypeResolver> = JsonDecodingVisitor<T>;

	fn into_visitor<T: TypeResolver>() -> JsonDecodingVisitor<T> {
		Default::default()
	}
}

/// Visitor implementation that delegates decoding to `RawJsonVisitor` and wraps the result in
/// `ScaleDecodedToJson`.
#[derive(Debug)]
pub struct JsonDecodingVisitor<R>(PhantomData<R>);

impl<R> Default for JsonDecodingVisitor<R> {
	fn default() -> Self {
		Self(Default::default())
	}
}

impl<R: TypeResolver> scale_decode::Visitor for JsonDecodingVisitor<R> {
	type Value<'scale, 'resolver> = ScaleDecodedToJson;
	type Error = scale_decode::Error;
	type TypeResolver = R;

	fn unchecked_decode_as_type<'scale, 'resolver>(
		self,
		input: &mut &'scale [u8],
		type_id: scale_decode::visitor::TypeIdFor<Self>,
		types: &'resolver Self::TypeResolver,
	) -> scale_decode::visitor::DecodeAsTypeResult<
		Self,
		Result<Self::Value<'scale, 'resolver>, Self::Error>,
	> {
		DecodeAsTypeResult::Decoded(
			decode_with_visitor(input, type_id, types, RawJsonVisitor::<R>::default())
				.map(Into::into)
				.map_err(Into::into),
		)
	}
}

/// A [Visitor](scale_decode::Visitor) that decodes scale encoded data into a raw JSON
/// [Value](serde_json::Value).
///
/// See the documentation of [ScaleDecodedToJson](crate::ScaleDecodedToJson) for more information.
#[derive(Debug)]
struct RawJsonVisitor<R>(PhantomData<R>);

impl<R> Copy for RawJsonVisitor<R> {}
impl<R> Clone for RawJsonVisitor<R> {
	fn clone(&self) -> Self {
		*self
	}
}
impl<R> Default for RawJsonVisitor<R> {
	fn default() -> Self {
		Self(Default::default())
	}
}

fn trimmed_hex<T: AsRef<[u8]>>(bytes: T) -> String {
	let s = hex::encode(bytes.as_ref());
	let trimmed = s.trim_start_matches('0');
	format!("0x{}", if trimmed.is_empty() { "0" } else { trimmed })
}

fn untrimmed_hex<T: AsRef<[u8]>>(bytes: T) -> String {
	format!("0x{}", hex::encode(bytes.as_ref()))
}

macro_rules! decode_to_number {
	( $(
		$name:ident, $t:ty
	),+ $(,)? ) => {
		$(
			fn $name<'scale, 'resolver>(
				self,
				value: $t,
				_type_id: scale_decode::visitor::TypeIdFor<Self>,
			) -> Result<Self::Value<'scale, 'resolver>, Self::Error> {
				Ok(serde_json::Value::Number(serde_json::Number::from(value)))
			}
		)+
	};
}

macro_rules! decode_to_hex {
	(
		$(
			$name:ident, $t:ty $( | $convert:ident )?
		),+
		$(,)?
	) => {
		$(
			fn $name<'scale, 'resolver>(
				self,
				value: $t,
				_type_id: scale_decode::visitor::TypeIdFor<Self>,
			) -> Result<Self::Value<'scale, 'resolver>, Self::Error> {
				Ok(serde_json::Value::String(trimmed_hex(value$( .$convert() )?)))
			}
		)+
	};
}

macro_rules! decode_to_array {
	(
		$(
			$name:ident, $scale_type:ident
		),+
		$(,)?
	) => {
		$(
			fn $name<'scale, 'resolver>(
			self,
			values: &mut scale_decode::visitor::types::$scale_type<'scale, 'resolver, Self::TypeResolver>,
			_type_id: scale_decode::visitor::TypeIdFor<Self>,
		) -> Result<Self::Value<'scale, 'resolver>, Self::Error> {
				Ok(serde_json::Value::Array(
					values.map(|res| {
						res.and_then(|item| {
							item.decode_with_visitor(self)
						})
					}).collect::<Result<Vec<_>,_>>()?
				))
			}
		)+
	};
}

macro_rules! decode_to_array_or_hex {
	(
		$(
			$name:ident, $scale_type:ident
		),+
		$(,)?
	) => {
		$(
			fn $name<'scale, 'resolver>(
			self,
			values: &mut scale_decode::visitor::types::$scale_type<'scale, 'resolver, Self::TypeResolver>,
			_type_id: scale_decode::visitor::TypeIdFor<Self>,
		) -> Result<Self::Value<'scale, 'resolver>, Self::Error> {
				// Heuristic to determine if the array is a byte array:
				// - Array must be non-empty
				// - AND Each value decodes to an unsigned json Number (ie. not an enum)
				// - AND each value was encoded as a single byte
				let (is_bytes, all_values) = values.try_fold(
					(values.remaining() > 0, Vec::with_capacity(values.remaining())),
					|(all_bytes, mut values), res| {
						res.and_then(|item| {
							let decoded_item = item.decode_with_visitor(self)?;
							let is_byte = item.bytes().len() == 1 && decoded_item.is_u64();
							values.push(decoded_item);
							Ok((all_bytes && is_byte, values))
						})
					},
				)?;
				if is_bytes {
					Ok(serde_json::Value::String(untrimmed_hex(
						all_values
							.into_iter()
							.map(|json| json.as_u64().unwrap() as u8)
							.collect::<Vec<u8>>(),
					)))
				} else {
					Ok(serde_json::Value::Array(all_values))
				}
			}
		)+
	};
}

impl<R: TypeResolver> scale_decode::Visitor for RawJsonVisitor<R> {
	type Value<'scale, 'resolver> = serde_json::Value;
	type Error = DecodeError;
	type TypeResolver = R;

	decode_to_number! {
		visit_u8, u8,
		visit_u16, u16,
		visit_u32, u32,
		visit_i8, i8,
		visit_i16, i16,
		visit_i32, i32,
	}

	decode_to_hex! {
		visit_u64, u64 | to_be_bytes,
		visit_u128, u128 | to_be_bytes,
		visit_u256, &'scale [u8; 32],
		visit_i64, i64 | to_be_bytes,
		visit_i128, i128 | to_be_bytes,
		visit_i256, &'scale [u8; 32],
	}

	decode_to_array! {
		visit_tuple, Tuple,
	}

	decode_to_array_or_hex! {
		visit_sequence, Sequence,
		visit_array, Array,
	}

	fn visit_bool<'scale, 'resolver>(
		self,
		value: bool,
		_type_id: scale_decode::visitor::TypeIdFor<Self>,
	) -> Result<Self::Value<'scale, 'resolver>, Self::Error> {
		Ok(serde_json::Value::Bool(value))
	}

	fn visit_char<'scale, 'resolver>(
		self,
		value: char,
		_type_id: scale_decode::visitor::TypeIdFor<Self>,
	) -> Result<Self::Value<'scale, 'resolver>, Self::Error> {
		Ok(serde_json::Value::String(value.to_string()))
	}

	fn visit_str<'scale, 'resolver>(
		self,
		value: &mut scale_decode::visitor::types::Str<'scale>,
		_type_id: scale_decode::visitor::TypeIdFor<Self>,
	) -> Result<Self::Value<'scale, 'resolver>, Self::Error> {
		Ok(serde_json::Value::String(value.as_str()?.to_string()))
	}

	fn visit_bitsequence<'scale, 'resolver>(
		self,
		value: &mut scale_decode::visitor::types::BitSequence<'scale>,
		_type_id: scale_decode::visitor::TypeIdFor<Self>,
	) -> Result<Self::Value<'scale, 'resolver>, Self::Error> {
		let s = value
			.decode()?
			.map(|r| r.map(|b| if b { '1' } else { '0' }))
			.collect::<Result<String, _>>()?;
		Ok(serde_json::Value::String(format!("0b{}", s)))
	}

	fn visit_composite<'scale, 'resolver>(
		self,
		value: &mut scale_decode::visitor::types::Composite<'scale, 'resolver, Self::TypeResolver>,
		_type_id: scale_decode::visitor::TypeIdFor<Self>,
	) -> Result<Self::Value<'scale, 'resolver>, Self::Error> {
		if value.has_unnamed_fields() {
			if value.remaining() == 1 {
				// Simple wrapper struct. No need for array.
				let field = value.next().unwrap()?;
				assert!(field.name().is_none(), "Checked for unnamed fields.");
				Ok(field.decode_with_visitor(RawJsonVisitor::<R>::default())?)
			} else {
				Ok(serde_json::Value::Array(
					value
						.map(|res| {
							res.and_then(|field| {
								let field_decoded =
									field.decode_with_visitor(RawJsonVisitor::<R>::default())?;
								if let Some(name) = field.name() {
									Ok(serde_json::Value::Object(
										std::iter::once((name.to_string(), field_decoded))
											.collect(),
									))
								} else {
									Ok(field_decoded)
								}
							})
						})
						.collect::<Result<Vec<_>, _>>()?,
				))
			}
		} else {
			Ok(serde_json::Value::Object(
				value
					.map(|res| {
						res.and_then(|field| {
							Ok((
								field.name().expect("Checked for unnamed fields.").to_string(),
								field.decode_with_visitor(RawJsonVisitor::<R>::default())?,
							))
						})
					})
					.collect::<Result<serde_json::Map<_, _>, _>>()?,
			))
		}
	}

	fn visit_variant<'scale, 'resolver>(
		self,
		value: &mut scale_decode::visitor::types::Variant<'scale, 'resolver, Self::TypeResolver>,
		_type_id: scale_decode::visitor::TypeIdFor<Self>,
	) -> Result<Self::Value<'scale, 'resolver>, Self::Error> {
		if value.fields().remaining() == 0 {
			Ok(serde_json::Value::String(value.name().to_string()))
		} else {
			Ok(serde_json::Value::Object(
				std::iter::once(value)
					.map(|value| {
						Ok::<_, Self::Error>((
							value.name().to_string(),
							RawJsonVisitor::<R>::default()
								.visit_composite(value.fields(), R::TypeId::default())?,
						))
					})
					.collect::<Result<serde_json::Map<_, _>, _>>()?,
			))
		}
	}
}
