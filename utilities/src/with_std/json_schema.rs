use schemars::{json_schema, Schema, SchemaGenerator};

const HEX_REGEX: &str = "^0x[0-9a-fA-F]*$";

pub fn hex_array<const LEN: u32>(_: &mut SchemaGenerator) -> Schema {
	json_schema!({
		"type": "string",
		"description": format!("A sequence of {} bytes encoded as a `0x`-prefixed hex string.", LEN),
		"pattern": HEX_REGEX,
		"minLength": LEN,
		"maxLength": LEN,
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
		"maxLength": MAX_LEN,
	})
}
pub fn number_or_hex(_: &mut SchemaGenerator) -> Schema {
	json_schema!(
		{
			"description": "A number represented as a JSON number or a `0x`-prefixed hex-encoded string.",
			"oneOf": [
				{
					"type": "string",
					"pattern": HEX_REGEX
				},
				{
					"type": "integer",
					"minimum": 0,
					"maximum": 2u64.pow(53) - 1,
				}
			]
		}
	)
}
pub fn set_max_length<const L: usize>(schema: &mut Schema) {
	schema.ensure_object().entry("maxLength").or_insert(L.into());
}

/// SS58 encoded account id.
#[allow(dead_code)] // We never construct the value but we need it as a placeholder for its JsonSchema.
#[derive(schemars::JsonSchema)]
#[schemars(transparent)]
pub struct AccountId32(String);
