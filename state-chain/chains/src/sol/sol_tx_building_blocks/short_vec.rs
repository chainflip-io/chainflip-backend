//! Compact serde-encoding of vectors with small length.

#![allow(clippy::arithmetic_side_effects)]
use serde::{
	de::{self, Deserializer, SeqAccess, Visitor},
	ser::{self, SerializeTuple, Serializer},
	Deserialize, Serialize,
};
use sp_std::{convert::TryFrom, fmt, marker::PhantomData, vec::Vec};

/// Same as u16, but serialized with 1 to 3 bytes. If the value is above
/// 0x7f, the top bit is set and the remaining value is stored in the next
/// bytes. Each byte follows the same pattern until the 3rd byte. The 3rd
/// byte, if needed, uses all 8 bits to store the last byte of the original
/// value.
pub struct ShortU16(pub u16);
impl Serialize for ShortU16 {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		// Pass a non-zero value to serialize_tuple() so that serde_json will
		// generate an open bracket.
		let mut seq = serializer.serialize_tuple(1)?;

		let mut rem_val = self.0;
		loop {
			let mut elem = (rem_val & 0x7f) as u8;
			rem_val >>= 7;
			if rem_val == 0 {
				seq.serialize_element(&elem)?;
				break
			} else {
				elem |= 0x80;
				seq.serialize_element(&elem)?;
			}
		}
		seq.end()
	}
}

enum VisitStatus {
	Done(u16),
	More(u16),
}

#[derive(Debug)]
enum VisitError {
	TooLong(usize),
	TooShort(usize),
	Overflow(u32),
	Alias,
	ByteThreeContinues,
}

impl VisitError {
	fn into_de_error<'de, A>(self) -> A::Error
	where
		A: SeqAccess<'de>,
	{
		match self {
			VisitError::TooLong(len) => de::Error::invalid_length(len, &"three or fewer bytes"),
			VisitError::TooShort(len) => de::Error::invalid_length(len, &"more bytes"),
			VisitError::Overflow(val) => de::Error::invalid_value(
				de::Unexpected::Unsigned(val as u64),
				&"a value in the range [0, 65535]",
			),
			VisitError::Alias => de::Error::invalid_value(
				de::Unexpected::Other("alias encoding"),
				&"strict form encoding",
			),
			VisitError::ByteThreeContinues => de::Error::invalid_value(
				de::Unexpected::Other("continue signal on byte-three"),
				&"a terminal signal on or before byte-three",
			),
		}
	}
}

type VisitResult = Result<VisitStatus, VisitError>;

const MAX_ENCODING_LENGTH: usize = 3;
fn visit_byte(elem: u8, val: u16, nth_byte: usize) -> VisitResult {
	if elem == 0 && nth_byte != 0 {
		return Err(VisitError::Alias)
	}

	let val = u32::from(val);
	let elem = u32::from(elem);
	let elem_val = elem & 0x7f;
	let elem_done = (elem & 0x80) == 0;

	if nth_byte >= MAX_ENCODING_LENGTH {
		return Err(VisitError::TooLong(nth_byte.saturating_add(1)))
	} else if nth_byte == MAX_ENCODING_LENGTH.saturating_sub(1) && !elem_done {
		return Err(VisitError::ByteThreeContinues)
	}

	let shift = u32::try_from(nth_byte).unwrap_or(u32::MAX).saturating_mul(7);
	let elem_val = elem_val.checked_shl(shift).unwrap_or(u32::MAX);

	let new_val = val | elem_val;
	let val = u16::try_from(new_val).map_err(|_| VisitError::Overflow(new_val))?;

	if elem_done {
		Ok(VisitStatus::Done(val))
	} else {
		Ok(VisitStatus::More(val))
	}
}

struct ShortU16Visitor;

impl<'de> Visitor<'de> for ShortU16Visitor {
	type Value = ShortU16;

	fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		formatter.write_str("a ShortU16")
	}

	fn visit_seq<A>(self, mut seq: A) -> Result<ShortU16, A::Error>
	where
		A: SeqAccess<'de>,
	{
		// Decodes an unsigned 16 bit integer one-to-one encoded as follows:
		// 1 byte  : 0xxxxxxx                   => 00000000 0xxxxxxx :      0 -    127
		// 2 bytes : 1xxxxxxx 0yyyyyyy          => 00yyyyyy yxxxxxxx :    128 - 16,383
		// 3 bytes : 1xxxxxxx 1yyyyyyy 000000zz => zzyyyyyy yxxxxxxx : 16,384 - 65,535
		let mut val: u16 = 0;
		for nth_byte in 0..MAX_ENCODING_LENGTH {
			let elem: u8 = seq.next_element()?.ok_or_else(|| {
				VisitError::TooShort(nth_byte.saturating_add(1)).into_de_error::<A>()
			})?;
			match visit_byte(elem, val, nth_byte).map_err(|e| e.into_de_error::<A>())? {
				VisitStatus::Done(new_val) => return Ok(ShortU16(new_val)),
				VisitStatus::More(new_val) => val = new_val,
			}
		}

		Err(VisitError::ByteThreeContinues.into_de_error::<A>())
	}
}

impl<'de> Deserialize<'de> for ShortU16 {
	fn deserialize<D>(deserializer: D) -> Result<ShortU16, D::Error>
	where
		D: Deserializer<'de>,
	{
		deserializer.deserialize_tuple(3, ShortU16Visitor)
	}
}

/// If you don't want to use the ShortVec newtype, you can do ShortVec
/// serialization on an ordinary vector with the following field annotation:
///
/// #[serde(with = "short_vec")]
pub fn serialize<S: Serializer, T: Serialize>(
	elements: &[T],
	serializer: S,
) -> Result<S::Ok, S::Error> {
	// Pass a non-zero value to serialize_tuple() so that serde_json will
	// generate an open bracket.
	let mut seq = serializer.serialize_tuple(1)?;

	let len = elements.len();
	if len > u16::MAX as usize {
		return Err(ser::Error::custom("length larger than u16"))
	}
	let short_len = ShortU16(len as u16);
	seq.serialize_element(&short_len)?;

	for element in elements {
		seq.serialize_element(element)?;
	}
	seq.end()
}

struct ShortVecVisitor<T> {
	_t: PhantomData<T>,
}

impl<'de, T> Visitor<'de> for ShortVecVisitor<T>
where
	T: Deserialize<'de>,
{
	type Value = Vec<T>;

	fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		formatter.write_str("a Vec with a multi-byte length")
	}

	fn visit_seq<A>(self, mut seq: A) -> Result<Vec<T>, A::Error>
	where
		A: SeqAccess<'de>,
	{
		let short_len: ShortU16 =
			seq.next_element()?.ok_or_else(|| de::Error::invalid_length(0, &self))?;
		let len = short_len.0 as usize;

		let mut result = Vec::with_capacity(len);
		for i in 0..len {
			let elem = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(i, &self))?;
			result.push(elem);
		}
		Ok(result)
	}
}

/// If you don't want to use the ShortVec newtype, you can do ShortVec
/// deserialization on an ordinary vector with the following field annotation:
///
/// #[serde(with = "short_vec")]
pub fn deserialize<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
	D: Deserializer<'de>,
	T: Deserialize<'de>,
{
	let visitor = ShortVecVisitor { _t: PhantomData };
	deserializer.deserialize_tuple(usize::MAX, visitor)
}

pub struct ShortVec<T>(pub Vec<T>);

impl<T: Serialize> Serialize for ShortVec<T> {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		serialize(&self.0, serializer)
	}
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for ShortVec<T> {
	fn deserialize<D>(deserializer: D) -> Result<ShortVec<T>, D::Error>
	where
		D: Deserializer<'de>,
	{
		deserialize(deserializer).map(ShortVec)
	}
}

/// Return the decoded value and how many bytes it consumed.
#[allow(clippy::result_unit_err)]
pub fn decode_shortu16_len(bytes: &[u8]) -> Result<(usize, usize), ()> {
	let mut val = 0;
	for (nth_byte, byte) in bytes.iter().take(MAX_ENCODING_LENGTH).enumerate() {
		match visit_byte(*byte, val, nth_byte).map_err(|_| ())? {
			VisitStatus::More(new_val) => val = new_val,
			VisitStatus::Done(new_val) =>
				return Ok((usize::from(new_val), nth_byte.saturating_add(1))),
		}
	}
	Err(())
}
