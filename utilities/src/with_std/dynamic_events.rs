use core::ops::Deref;
use itertools::Itertools;
use scale_info::{
	scale::{Compact, Decode},
	PortableRegistry,
};
use scale_json::{
	ext::{DecodeAsType, JsonValue},
	Error as DecodeError, ScaleDecodedToJson,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// The fully-scoped type name of the events that are stored in the System pallet.
const SYSTEM_EVENTS_TYPE_NAME: &str = "frame_system::EventRecord";

#[derive(Error, Debug, Clone)]
pub enum DispatchError {
	#[error("Dispatch error: {0}")]
	DispatchError(JsonValue),
	#[error("Module error ‘{name}‘ from pallet ‘{pallet}‘: ‘{error}‘")]
	KnownModuleError { pallet: String, name: String, error: String },
}

#[derive(Debug, Serialize, Deserialize)]
/// A wrapper around a JSON value that represents a substrate EventRecord.
pub struct DynamicEventRecord(JsonValue);

impl Deref for DynamicEventRecord {
	type Target = JsonValue;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[derive(Debug)]
pub struct DynamicEvent(JsonValue);

impl Deref for DynamicEvent {
	type Target = JsonValue;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl DynamicEventRecord {
	/// Wrap a JSON value that represents a substrate EventRecord.
	pub fn new(json: JsonValue) -> Option<Self> {
		if json.as_object().map_or(false, |obj| {
			obj.contains_key("event") && obj.contains_key("phase") && obj.contains_key("topics")
		}) {
			Some(Self(json))
		} else {
			None
		}
	}

	/// Returns the inner event if it matches the given extrinsic index.
	pub fn extrinsic_event(&self, extrinsic_index: u64) -> Option<DynamicEvent> {
		self.0
			.pointer("/phase/ApplyExtrinsic")
			.and_then(|index| index.as_u64())
			.filter(|index| *index == extrinsic_index)
			.map(|_| self.event())
	}

	/// Returns the inner event and it's extrinsic index if the event was triggered during the
	/// ApplyExtrinsic phase.
	pub fn indexed_extrinsic_event(&self) -> Option<(u64, DynamicEvent)> {
		self.0
			.pointer("/phase/ApplyExtrinsic")
			.and_then(|index| index.as_u64())
			.map(|index| (index, self.event()))
	}

	/// Return a clone of the inner event.
	pub fn event(&self) -> DynamicEvent {
		DynamicEvent(
			self.0
				.pointer("/event")
				.expect("existence of /event is ensured by constructor")
				.clone(),
		)
	}

	/// Unwrap the inner JSON value.
	pub fn into_inner(self) -> JsonValue {
		self.0
	}
}

impl DynamicEvent {
	/// If this event represents the outcome of an extrinsic, return the result.
	pub fn extrinsic_outcome(&self) -> Option<Result<(), DispatchError>> {
		if self.0.pointer("/System/ExtrinsicSuccess/dispatch_info").is_some() {
			Some(Ok(()))
		} else {
			self.0
				.pointer("/System/ExtrinsicFailed/dispatch_error")
				.map(|dispatch_error_json| {
					Err((|| {
						Some(DispatchError::KnownModuleError {
							pallet: dispatch_error_json["pallet"].as_str()?.to_string(),
							name: dispatch_error_json["name"].as_str()?.to_string(),
							error: dispatch_error_json["error"].as_str()?.to_string(),
						})
					})()
					.unwrap_or_else(|| DispatchError::DispatchError(dispatch_error_json.clone())))
				})
		}
	}

	pub fn to_json(self) -> JsonValue {
		self.0.clone()
	}
}

type NamedVariantLookup<T> = BTreeMap<u8, (String, T)>;
type ErrorInfo = NamedVariantLookup<Vec<String>>;

/// A decoder for substrate events.
///
/// Events are decoded dynamically based on metadata provided on construction of this type.
///
/// For the System.ExtrinsicFailed event, the dispatch error is resolved to a structured
/// human-readable error message with the following shape:
///
/// ```json
/// {
///     "pallet": "PalletName",
///     "name": "ErrorName",
///     "error": "Human readable error message"
/// }
/// ```
#[derive(Debug)]
pub struct EventDecoder {
	events_type_id: u32,
	types: PortableRegistry,
	errors: NamedVariantLookup<ErrorInfo>,
}

impl EventDecoder {
	/// Create a new event decoder.
	///
	/// The arguments to this constructor can be extracted from substrate metadata.
	pub fn new(mut types: PortableRegistry, errors_type_id: u32) -> Self {
		let errors = match types.resolve(errors_type_id).unwrap().type_def.clone() {
			scale_info::TypeDef::Variant(runtime_error_type) => runtime_error_type
				.variants
				.into_iter()
				.map(|pallet_error_type| {
					(
						pallet_error_type.index,
						(pallet_error_type.name, {
							let type_id =
								pallet_error_type.fields.iter().exactly_one().unwrap().ty.id;
							match types.resolve(type_id).unwrap().type_def.clone() {
								scale_info::TypeDef::Variant(pallet_errors) => pallet_errors
									.variants
									.into_iter()
									.map(|error_variant| {
										(
											error_variant.index,
											(error_variant.name, error_variant.docs),
										)
									})
									.collect::<ErrorInfo>(),
								_ => panic!("Inner error type is not an Enum"),
							}
						}),
					)
				})
				.collect::<NamedVariantLookup<_>>(),
			_ => panic!("Outer error type is not an Enum"),
		};

		let events_type_id = types
			.types
			.iter()
			.find(|t| t.ty.path.segments.join("::") == SYSTEM_EVENTS_TYPE_NAME)
			.map(|t| t.id)
			.expect("System events type must exist in the metadata");

		let events_type_id = *types
			.retain(|id| events_type_id == id)
			.get(&events_type_id)
			.expect("outer event enum must exist in the metadata");

		debug_assert_eq!(
			types
				.resolve(events_type_id)
				.expect("events_type_id found in types above, so it must exist")
				.path
				.segments
				.join("::"),
			SYSTEM_EVENTS_TYPE_NAME
		);

		log::debug!("EventDecoder error lookup: {:?}", errors);

		Self { events_type_id, types, errors }
	}

	/// Decode a raw buffer of bytes into a list of [DyanmicEventRecord]s.
	///
	/// The input should be the raw scale encoded bytes queried from `System.Events` storage.
	pub fn decode_events(
		&self,
		events_data: Vec<u8>,
	) -> Result<Vec<DynamicEventRecord>, DecodeError> {
		let events_data_cursor = &mut &events_data[..];

		// The data represents a Vec<EventRecord<T, H>>, so it's prefixed with a Compact<u32>
		// denoting the length of the Vec (ie. the number of events).
		let event_count = Compact::<u32>::decode(events_data_cursor)
			.expect("Failed to decode CompactLen")
			.0;

		let initial_len = events_data_cursor.len();
		let decoded = (0..event_count)
			.map(|_| {
				ScaleDecodedToJson::decode_as_type(
					events_data_cursor,
					self.events_type_id,
					&self.types,
				)
				.and_then(|decoded| {
					let mut json: JsonValue = decoded.into();
					// This translates the raw error into a human-readable error message using
					// the stored error metadata (see Self::resolve_dispatch_error).
					if let Some(dispatch_error) =
						json.pointer_mut("/event/System/ExtrinsicFailed/dispatch_error")
					{
						if let Some(decoded_error) =
							self.resolve_dispatch_error(dispatch_error.clone())
						{
							*dispatch_error = decoded_error;
						} else {
							log::debug!("Failed to resolve dispatch error: {}", *dispatch_error)
						}
					}

					log::debug!("Decoded event: {}", json);

					DynamicEventRecord::new(json)
						.ok_or(DecodeError::custom_str("Invalid event record"))
				})
				.inspect(|json| log::debug!("Decoded event: {}", **json))
				.inspect_err(|e| {
					log::error!(
						"Failed to decode event at data index {}: {}",
						events_data_cursor.len() - initial_len,
						e
					)
				})
			})
			.try_collect()?;

		log::debug!("Decoded {} events", event_count);
		log::debug!("Remaining bytes: {}", events_data_cursor.len());

		assert_eq!(
			events_data_cursor.len(),
			0,
			"All events should be decoded and remaining bytes should be 0"
		);

		Ok(decoded)
	}

	/// Resolve a dispatch error to a structured human-readable error message.
	fn resolve_dispatch_error(&self, dispatch_error: JsonValue) -> Option<JsonValue> {
		log::debug!("Resolving dispatch error: {:?}", dispatch_error,);

		let error_index = u8::decode(
			&mut hex::decode(
				dispatch_error
					.pointer("/Module/error")?
					.as_str()
					.expect("/Module/error must be a hex string")
					.trim_start_matches("0x"),
			)
			.expect("/Module/error must be a hex string")
			.as_slice(),
		)
		.expect("Error index must fit in a u8");

		log::debug!("Module error index: {}", error_index);

		let pallet_index = dispatch_error
			.pointer("/Module/index")?
			.as_u64()
			.expect("/Module/index must be a number that fits in a u8") as u8;

		log::debug!("Pallet index: {}", pallet_index);

		self.errors.get(&pallet_index).and_then(|(pallet, pallet_errors)| {
			pallet_errors.get(&error_index).map(|(name, doc)| {
				JsonValue::Object(
					[("pallet", pallet.clone()), ("name", name.clone()), ("error", doc.join(" "))]
						.into_iter()
						.map(|(k, v)| (k.to_string(), JsonValue::String(v)))
						.collect(),
				)
			})
		})
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_event_wrappers() {
		let invalid = serde_json::json!({
			"phase": {
				"ApplyExtrinsic": 401
			},
			"topics": "0x"
		});
		assert!(DynamicEventRecord::new(invalid).is_none());

		let success = serde_json::json!({
			"event": {
				"System": {
					"ExtrinsicSuccess": {
						"dispatch_info": {
							"class": "Normal",
							"pays_fee": "Yes",
							"weight": {
								"proof_size": "0x1efd",
								"ref_time": "0x282deb50"
							}
						}
					},
				}
			},
			"phase": {
				"ApplyExtrinsic": 401
			},
			"topics": "0x"
		});
		let raw_error: serde_json::Value = serde_json::json!({
			"Module": {
				"error": "0x00000000",
				"index": 31
			}
		});
		let failed_1 = serde_json::json!({
			"event": {
				"System": {
					"ExtrinsicFailed": {
						"dispatch_error": raw_error,
						"dispatch_info": {
							"class": "Normal",
							"pays_fee": "Yes",
							"weight": {
								"proof_size": "0x1efd",
								"ref_time": "0x282deb50"
							}
						}
					}
				}
			},
			"phase": {
				"ApplyExtrinsic": 401
			},
			"topics": "0x"
		});
		const ERROR: &str = "The user does not have enough funds.";
		const ERROR_PALLET: &str = "LiquidityProvider";
		const ERROR_NAME: &str = "InsufficientBalance";
		let failed_2 = serde_json::json!({
			"event": {
				"System": {
					"ExtrinsicFailed": {
						"dispatch_error":{
							"error": ERROR,
							"name": ERROR_NAME,
							"pallet": ERROR_PALLET
						},
						"dispatch_info": {
							"class": "Normal",
							"pays_fee": "Yes",
							"weight": {
								"proof_size": "0x1efd",
								"ref_time": "0x282deb50"
							}
						}
					}
				}
			},
			"phase": {
				"ApplyExtrinsic": 401
			},
			"topics": "0x"
		});

		for json in [&success, &failed_1, &failed_2] {
			let record = DynamicEventRecord::new(json.clone()).unwrap();
			let (index, event) = record.indexed_extrinsic_event().unwrap();
			assert_eq!(index, 401);
			assert_eq!(record.extrinsic_event(index).as_deref(), Some(event).as_deref());
		}

		#[track_caller]
		fn outcome(json: JsonValue) -> Result<(), DispatchError> {
			DynamicEventRecord::new(json)
				.expect("Valid json event record")
				.event()
				.extrinsic_outcome()
				.expect("outcome should not be None")
		}

		assert!(outcome(success).is_ok());
		assert!(
			matches!(outcome(failed_1.clone()).unwrap_err(), DispatchError::DispatchError(e) if e == raw_error),
			"Expected dispatch error to be DispatchError::DispatchError({:?}), got {:?}",
			raw_error,
			outcome(failed_1),
		);
		assert!(matches!(
			outcome(failed_2).unwrap_err(),
			DispatchError::KnownModuleError {
				pallet,
				name,
				error
			} if pallet == ERROR_PALLET && name == ERROR_NAME && error == ERROR
		));
	}
}
