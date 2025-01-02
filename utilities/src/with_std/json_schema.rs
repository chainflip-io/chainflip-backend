use schemars::{json_schema, Schema, SchemaGenerator};

const HEX_REGEX: &str = "^0x[0-9a-fA-F]+$";
const BASE58_REGEX: &str = "^[1-9A-HJ-NP-Za-km-z]+$";

/// Returns the maximum length of a hex string for the given *byte* length.
///
/// This is `(byte_length * 2) + 2`. The `+ 2` is for the optional `0x`.
pub const fn hex_string_len<const LEN: u32>() -> u32 {
	(LEN * 2) + 2
}
fn base58_len(n: usize) -> usize {
	((n * 8) as f64 / f64::from(58_u32).log2()).ceil() as usize
}

pub fn hex_array<const LEN: u32>(_: &mut SchemaGenerator) -> Schema {
	json_schema!({
		"type": "string",
		"description": format!("A sequence of {} bytes encoded as a `0x`-prefixed hex string.", LEN),
		"pattern": HEX_REGEX,
		"minLength": hex_string_len::<LEN>(),
		"maxLength": hex_string_len::<LEN>(),
	})
}
pub fn hex_vec(_: &mut SchemaGenerator) -> Schema {
	json_schema!({
		"type": "string",
		"description": "A vector of bytes encoded as a `0x`-prefixed hex string.",
		"pattern": HEX_REGEX,
	})
}
pub fn bounded_hex_vec<const MAX_LEN: u32>(_: &mut SchemaGenerator) -> Schema {
	json_schema!({
		"type": "string",
		"description": format!("A sequence of at most {} bytes encoded as a `0x`-prefixed hex string.", MAX_LEN),
		"pattern": HEX_REGEX,
		"maxLength": hex_string_len::<MAX_LEN>(),
	})
}
pub fn number_or_hex(gen: &mut SchemaGenerator) -> Schema {
	json_schema!(
		{
			"description": "A number represented as a JSON number or a `0x`-prefixed hex-encoded string.",
			"oneOf": [
				u256_hex(gen),
				{
					"type": "integer",
					"minimum": 0,
					"maximum": 2u64.pow(53) - 1,
				}
			]
		}
	)
}
pub fn u256_hex(_: &mut SchemaGenerator) -> Schema {
	json_schema!(
		{
			"description": "A number represented a `0x`-prefixed hex-encoded string.",
			"type": "string",
			"pattern": HEX_REGEX,
			"maxLength": hex_string_len::<32>(),
		}
	)
}
pub fn base58_bytes(_: &mut SchemaGenerator) -> Schema {
	json_schema!({
		"type": "string",
		"description": "A string of base58-encoded bytes.",
		"pattern": BASE58_REGEX,
	})
}
pub fn base58_array<const LEN: usize>(_: &mut SchemaGenerator) -> Schema {
	json_schema!({
		"type": "string",
		"description": format!("A sequence of exactly {} base58-encoded bytes ({} characters).", LEN, base58_len(LEN)),
		"pattern": BASE58_REGEX,
	})
}
pub fn set_max_length<const L: usize>(schema: &mut Schema) {
	schema.ensure_object().entry("maxLength").or_insert(L.into());
}

/// SS58 encoded account id.
#[allow(dead_code)] // We never construct the value but we need it as a placeholder for its JsonSchema.
#[derive(schemars::JsonSchema)]
#[schemars(transparent)]
pub struct AccountId32(String);
