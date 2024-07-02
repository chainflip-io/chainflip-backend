//! A dynamic decoder for substrate events.
//!
//! This module provides a decoder for substrate events that can be used to decode events from
//! raw scale-encoded bytes, provided some compatible metadata.
//!
//! In addition to the [EventDecoder] itself, some wrapper types are provided to make working with
//! the decoded events easier. See for example [DynamicEventRecord] and [DynamicEvent].
use anyhow::{anyhow, Context};
use core::{ops::Deref, str::FromStr};
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

pub type DynamicDispatchError = ResolvedDispatchError<JsonValue>;

/// A dispatch error whose inner ModuleError indices have been resolved to a human-readable error,
/// if possible.
///
/// If the error did not contain a module error, or if the module error could not be resolved, this
/// contains the original dispatch error.
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum ResolvedDispatchError<E> {
	#[error("Dispatch error: {0:?}")]
	DispatchError(E),
	#[error("Module error ‘{name}‘ from pallet ‘{pallet}‘: ‘{error}‘")]
	KnownModuleError { pallet: String, name: String, error: String },
}

impl<E> From<ErrorInfo> for ResolvedDispatchError<E> {
	fn from(info: ErrorInfo) -> Self {
		ResolvedDispatchError::KnownModuleError {
			pallet: info.pallet,
			name: info.name,
			error: info.error,
		}
	}
}

impl ResolvedDispatchError<JsonValue> {
	pub fn from_error_json(dispatch_error_json: &JsonValue) -> Self {
		(|| {
			Some(ResolvedDispatchError::KnownModuleError {
				pallet: dispatch_error_json["pallet"].as_str()?.to_string(),
				name: dispatch_error_json["name"].as_str()?.to_string(),
				error: dispatch_error_json["error"].as_str()?.to_string(),
			})
		})()
		.unwrap_or_else(|| ResolvedDispatchError::DispatchError(dispatch_error_json.clone()))
	}
}

pub trait WrappedJsonValue: Clone + AsRef<JsonValue> + Into<JsonValue> {
	fn as_json(&self) -> &JsonValue {
		self.as_ref()
	}
	fn try_deserialize_into<T: serde::de::DeserializeOwned>(self) -> Result<T, serde_json::Error> {
		serde_json::from_value(self.into())
	}
}

impl<T: Clone + AsRef<JsonValue> + Into<JsonValue>> WrappedJsonValue for T {}

macro_rules! impl_wrapped_json_value {
	( $name:ident ) => {
		#[derive(Debug, Clone, Serialize, Deserialize)]
		pub struct $name(JsonValue);

		impl AsRef<JsonValue> for $name {
			fn as_ref(&self) -> &JsonValue {
				&self.0
			}
		}

		impl Deref for $name {
			type Target = JsonValue;

			fn deref(&self) -> &Self::Target {
				&self.0
			}
		}

		impl $name {
			pub fn try_deserialize_into<T: serde::de::DeserializeOwned>(
				self,
			) -> Result<T, EventDeserializationError<'static>> {
				serde_json::from_value(self.0).map_err(Into::into)
			}

			pub fn try_deserialize_item_into<'a, T: serde::de::DeserializeOwned>(
				&self,
				pointer: &'a str,
			) -> Result<T, EventDeserializationError<'a>> {
				serde_json::from_value(
					self.0
						.pointer(pointer)
						.ok_or_else(|| EventDeserializationError::PathError(pointer))?
						.clone(),
				)
				.map_err(Into::into)
			}
		}
	};
}

impl_wrapped_json_value!(DynamicEventRecord);
impl_wrapped_json_value!(DynamicRuntimeEvent);
impl_wrapped_json_value!(DynamicPalletEvent);

impl DynamicEventRecord {
	/// Wrap a JSON value that represents a substrate EventRecord.
	fn new(json: JsonValue) -> Option<Self> {
		if json.as_object().map_or(false, |obj| {
			obj.contains_key("event") && obj.contains_key("phase") && obj.contains_key("topics")
		}) {
			Some(Self(json))
		} else {
			None
		}
	}

	/// Returns the inner event if it matches the given extrinsic index.
	pub fn extrinsic_event(&self, extrinsic_index: u64) -> Option<DynamicRuntimeEvent> {
		self.0
			.pointer("/phase/ApplyExtrinsic")
			.and_then(|index| index.as_u64())
			.filter(|index| *index == extrinsic_index)
			.map(|_| self.event())
	}

	/// Returns the inner event and it's extrinsic index if the event was triggered during the
	/// ApplyExtrinsic phase.
	pub fn indexed_extrinsic_event(&self) -> Option<(u64, DynamicRuntimeEvent)> {
		self.0
			.pointer("/phase/ApplyExtrinsic")
			.and_then(|index| index.as_u64())
			.map(|index| (index, self.event()))
	}

	/// Return a clone of the inner event.
	pub fn event(&self) -> DynamicRuntimeEvent {
		DynamicRuntimeEvent(
			self.0
				.pointer("/event")
				.expect("existence of /event is ensured by constructor")
				.clone(),
		)
	}
}

#[derive(Error, Debug)]
pub enum EventDeserializationError<'a> {
	#[error("JSON error: {0}")]
	JsonError(#[from] serde_json::Error),
	#[error("Unknown path: {0}")]
	PathError(&'a str),
}

impl DynamicRuntimeEvent {
	/// If this event represents the outcome of an extrinsic, return the result.
	pub fn extrinsic_outcome(&self) -> Option<Result<(), ResolvedDispatchError<JsonValue>>> {
		if self.0.pointer("/System/ExtrinsicSuccess/dispatch_info").is_some() {
			Some(Ok(()))
		} else {
			self.0
				.pointer("/System/ExtrinsicFailed/dispatch_error")
				.map(|dispatch_error_json| {
					Err(ResolvedDispatchError::from_error_json(dispatch_error_json))
				})
		}
	}

	/// If this event's pallet and event name match those provided, return the [DynamicPalletEvent].
	///
	/// Example:
	///
	/// ```
	/// let event = DynamicRuntimeEvent(serde_json::json!({
	///   "System": {
	///     "ExtrinsicSuccess": {
	///       "dispatch_info": {
	///         "class": "Normal",
	///         "pays_fee": "Yes"
	///       }
	///     }
	///   }
	/// }));
	/// assert!(event.pallet_event("System", "ExtrinsicSuccess").is_some());
	/// ```
	pub fn pallet_event(&self, pallet: &str, event: &str) -> Option<DynamicPalletEvent> {
		self.0
			.pointer(format!("/{}/{}", pallet, event).as_str())
			.cloned()
			.map(DynamicPalletEvent)
	}
}

type NamedVariantLookup<T> = BTreeMap<u8, (String, T)>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorLookup {
	/// A lookup table for dispatch errors.
	///
	/// The outer lookup is by pallet index, the inner lookup is by error index.
	errors: NamedVariantLookup<NamedVariantLookup<Vec<String>>>,
}

/// Structured information derived from a ModuleError.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInfo {
	pub pallet: String,
	pub name: String,
	pub error: String,
}

impl ErrorLookup {
	pub fn new(errors: NamedVariantLookup<NamedVariantLookup<Vec<String>>>) -> Self {
		Self { errors }
	}

	pub fn lookup(&self, pallet_index: u8, error_index: u8) -> Option<ErrorInfo> {
		self.errors.get(&pallet_index).and_then(|(pallet, pallet_errors)| {
			pallet_errors.get(&error_index).map(|(name, doc)| ErrorInfo {
				pallet: pallet.clone(),
				name: name.clone(),
				error: doc.join(" "),
			})
		})
	}
}

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
	pub errors: ErrorLookup,
}

impl EventDecoder {
	/// Create a new event decoder.
	///
	/// The arguments to this constructor can be extracted from substrate metadata.
	pub fn new(mut types: PortableRegistry, errors_type_id: u32) -> Self {
		let errors =
			ErrorLookup::new(match types.resolve(errors_type_id).unwrap().type_def.clone() {
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
										.collect::<NamedVariantLookup<_>>(),
									_ => panic!("Inner error type is not an Enum"),
								}
							}),
						)
					})
					.collect::<NamedVariantLookup<_>>(),
				_ => panic!("Expected the RuntimeError type, which should be an Enum."),
			});

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

		self.errors.lookup(pallet_index, error_index).map(|e| {
			serde_json::to_value(e)
				.expect("ErrorInfo is serializable to JSON, so this should never fail")
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
		fn outcome(json: JsonValue) -> Result<(), ResolvedDispatchError<JsonValue>> {
			DynamicEventRecord::new(json)
				.expect("Valid json event record")
				.event()
				.extrinsic_outcome()
				.expect("outcome should not be None")
		}

		assert!(outcome(success).is_ok());
		assert!(
			matches!(outcome(failed_1.clone()).unwrap_err(), ResolvedDispatchError::DispatchError(e) if e == raw_error),
			"Expected dispatch error to be ResolvedDispatchError::DispatchError({:?}), got {:?}",
			raw_error,
			outcome(failed_1),
		);
		assert!(matches!(
			outcome(failed_2).unwrap_err(),
			ResolvedDispatchError::KnownModuleError {
				pallet,
				name,
				error
			} if pallet == ERROR_PALLET && name == ERROR_NAME && error == ERROR
		));
	}
}

/// Extension trait for [JsonValue].
///
/// Allows decoding encoding/decoding values of type [JsonValue] without having to import
/// serde_json.
pub trait JsonExt: Sized + _seal::Sealed {
	fn from_json_str(json_str: &str) -> anyhow::Result<Self>;
	fn try_deserialize_into<T: serde::de::DeserializeOwned>(&self) -> anyhow::Result<T>;
	fn try_from_hex<E, T>(&self) -> anyhow::Result<T>
	where
		E: std::error::Error + Send + Sync + 'static,
		T: TryFrom<Vec<u8>, Error = E>;
	fn try_parse_from_str<T, E>(&self) -> anyhow::Result<T>
	where
		E: std::error::Error + Send + Sync + 'static,
		T: FromStr<Err = E>;
}

mod _seal {
	pub trait Sealed {}
}

impl _seal::Sealed for JsonValue {}
impl JsonExt for JsonValue {
	fn from_json_str(json_str: &str) -> anyhow::Result<Self> {
		serde_json::from_str(json_str).map_err(Into::into)
	}
	fn try_deserialize_into<T: serde::de::DeserializeOwned>(&self) -> anyhow::Result<T> {
		serde_json::from_value(self.clone()).map_err(Into::into)
	}
	fn try_from_hex<E, T>(&self) -> anyhow::Result<T>
	where
		E: std::error::Error + Send + Sync + 'static,
		T: TryFrom<Vec<u8>, Error = E>,
	{
		let str = self.as_str().ok_or_else(|| anyhow!("Expected a JSON string"))?;
		let bytes =
			hex::decode(str.trim_start_matches("0x")).context("Failed to decode hex string")?;
		Ok(T::try_from(bytes)?)
	}
	fn try_parse_from_str<T, E>(&self) -> anyhow::Result<T>
	where
		E: std::error::Error + Send + Sync + 'static,
		T: FromStr<Err = E>,
	{
		Ok(str::parse(self.as_str().ok_or_else(|| anyhow!("Expected a JSON string"))?)?)
	}
}
