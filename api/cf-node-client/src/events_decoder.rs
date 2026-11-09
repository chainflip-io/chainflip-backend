// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use crate::subxt_state_chain_config::{cf_static_runtime, StateChainConfig};
use codec::Decode;
use frame_system::DispatchEventInfo;
use scale_info::{form::PortableForm, Variant};
use sp_runtime::{DispatchError, Either};
use std::sync::Arc;
use subxt::{
	client::OfflineClientAtBlock, error::EventsError, events::DecodeAsEvent, ext::scale_decode,
	metadata::PalletMetadata, ArcMetadata, OfflineClient,
};

#[derive(thiserror::Error, Debug)]
pub enum DynamicEventError {
	#[error(transparent)]
	SubxtError(Box<subxt::Error>),
	#[error("Unexpected chain behaviour, ExtrinsicSuccess or ExtrinsicFailed event not found.")]
	UnexpectedChainBehaviour,
	#[error("Could not decode event, it might be because you running an old binary please consider upgrading. {0}")]
	EventDecodeError(String),
	#[error("Event unknown to static metadata, it might be because you running an old binary please consider upgrading your node")]
	EventUnknownToStaticMetadata,
	#[error("{0} event was not found")]
	StaticEventNotFound(&'static str),
}

impl From<subxt::Error> for DynamicEventError {
	fn from(e: subxt::Error) -> Self {
		DynamicEventError::SubxtError(Box::new(e))
	}
}

impl From<EventsError> for DynamicEventError {
	fn from(e: EventsError) -> Self {
		subxt::Error::from(e).into()
	}
}

/// An event decoded from a block's event records. Owns its bytes so that it can outlive the
/// [subxt::events::Events] it was decoded from.
#[derive(Debug, Clone)]
pub struct DynamicEvent {
	pallet_name: String,
	event_name: String,
	pallet_index: u8,
	event_index: u8,
	field_bytes: Vec<u8>,
	current_metadata: ArcMetadata,
	static_metadata: ArcMetadata,
}

impl DynamicEvent {
	fn event_metadata(
		metadata: &subxt::Metadata,
		pallet_index: u8,
		event_index: u8,
	) -> Option<(PalletMetadata<'_>, &Variant<PortableForm>)> {
		let pallet = metadata.pallet_by_event_index(pallet_index)?;
		let variant = pallet.event_variant_by_index(event_index)?;
		Some((pallet, variant))
	}

	fn decode_fields<E: DecodeAsEvent>(
		&self,
		bytes: &mut &[u8],
		variant: &Variant<PortableForm>,
		metadata: &subxt::Metadata,
	) -> Result<E, DynamicEventError> {
		let mut fields = variant
			.fields
			.iter()
			.map(|f| scale_decode::Field::new(f.ty.id, f.name.as_deref()));
		E::decode_as_fields(bytes, &mut fields, metadata.types()).map_err(|reason| {
			EventsError::CannotDecodeEventFields {
				pallet_name: self.pallet_name.clone(),
				event_name: self.event_name.clone(),
				reason,
			}
			.into()
		})
	}

	pub fn as_event<E: DecodeAsEvent>(&self) -> Result<Option<E>, DynamicEventError> {
		if !E::is_event(&self.pallet_name, &self.event_name) {
			return Ok(None);
		}
		let (_, variant) =
			Self::event_metadata(&self.current_metadata, self.pallet_index, self.event_index)
				.ok_or(EventsError::CannotFindVariantWithIndex {
					pallet_name: self.pallet_name.clone(),
					variant_index: self.event_index,
				})?;
		self.decode_fields(&mut &self.field_bytes[..], variant, &self.current_metadata)
			.map(Some)
	}

	pub fn as_event_strict<E: DecodeAsEvent>(&self) -> Result<Option<E>, DynamicEventError> {
		if !E::is_event(&self.pallet_name, &self.event_name) {
			return Ok(None);
		}
		let (_, current_variant) =
			Self::event_metadata(&self.current_metadata, self.pallet_index, self.event_index)
				.ok_or(EventsError::CannotFindVariantWithIndex {
					pallet_name: self.pallet_name.clone(),
					variant_index: self.event_index,
				})?;

		// Make sure the event is still known to the static metadata. i.e. it was not removed in a
		// newer runtime version
		let static_variant = self
			.static_metadata
			.pallet_by_name(&self.pallet_name)
			.and_then(|pallet| pallet.event_variant_by_index(self.event_index))
			.filter(|variant| variant.name == self.event_name)
			.ok_or(DynamicEventError::EventUnknownToStaticMetadata)?;

		// Decode the fields using the old static metadata
		let mut bytes = &self.field_bytes[..];
		let decoded = self.decode_fields(&mut bytes, static_variant, &self.static_metadata)?;

		// **STRICT NAMES CHECK **: Ensure decoded fields match old static metadata fields
		let static_field_names: Vec<_> =
			static_variant.fields.iter().map(|f| f.name.as_deref()).collect();

		let actual_field_names: Vec<_> =
			current_variant.fields.iter().map(|f| f.name.as_deref()).collect();

		if static_field_names != actual_field_names {
			return Err(DynamicEventError::EventDecodeError(format!(
				"{} event strict fields check failed: expected {:?}, got {:?}",
				self.event_name, static_field_names, actual_field_names
			)));
		}

		// **STRICT BYTE CHECK**: Ensure no extra bytes remain
		if !bytes.is_empty() {
			return Err(DynamicEventError::EventDecodeError(format!(
				"{} event strict byte check failed",
				self.event_name
			)));
		}

		Ok(Some(decoded))
	}

	pub fn pallet_name(&self) -> &str {
		&self.pallet_name
	}

	pub fn variant_name(&self) -> &str {
		&self.event_name
	}
}

#[derive(Debug, Clone)]
pub struct DynamicEvents {
	events: Vec<DynamicEvent>,
}

impl Iterator for DynamicEvents {
	type Item = DynamicEvent;

	fn next(&mut self) -> Option<Self::Item> {
		self.events.pop()
	}
}

impl DynamicEvents {
	pub fn event_names(&self) -> Vec<String> {
		self.events
			.iter()
			.map(|event| format!("{}::{}", event.pallet_name(), event.variant_name()))
			.collect()
	}

	/// Finds the first event of type E, parameter `is_strict` determines if the event should be
	/// decoded strictly.
	///   * True: Performs checks that the event fields match the static metadata fields and that no
	///     extra bytes remain.
	///   * False: leniently tries to decode the event, it doesn't fail if extra fields are added to
	///     the event.
	pub fn find_static_event<E: DecodeAsEvent>(
		&self,
		is_strict: bool,
	) -> Result<Option<E>, DynamicEventError> {
		for event in self.events.iter() {
			if E::is_event(event.pallet_name(), event.variant_name()) {
				return if is_strict { event.as_event_strict::<E>() } else { event.as_event::<E>() }
			}
		}
		Ok(None)
	}

	/// Finds all events of type E, parameter `is_strict` determines if the events should be decoded
	/// strictly.
	pub fn find_all_static_events<E: DecodeAsEvent>(
		&self,
		is_strict: bool,
	) -> Result<Vec<E>, DynamicEventError> {
		self.events
			.iter()
			.filter(|event| E::is_event(event.pallet_name(), event.variant_name()))
			.filter_map(|event| {
				if is_strict {
					event.as_event_strict::<E>().transpose()
				} else {
					event.as_event::<E>().transpose()
				}
			})
			.collect()
	}

	/// Extracts the extrinsic Success or Failed events
	pub fn extrinsic_result(
		&self,
	) -> Result<Either<DispatchEventInfo, DispatchError>, DynamicEventError> {
		for event in self.events.iter() {
			match (event.pallet_name(), event.variant_name()) {
				("System", "ExtrinsicSuccess") => {
					let eve = event
						.as_event::<cf_static_runtime::system::events::ExtrinsicSuccess>()?
						.unwrap();
					let dispatch_info = eve.dispatch_info;
					return Ok(Either::Left(dispatch_info.into()))
				},
				("System", "ExtrinsicFailed") => {
					let eve = event
						.as_event::<cf_static_runtime::system::events::ExtrinsicFailed>()?
						.unwrap();
					let dispatch_error = eve.dispatch_error;

					return Ok(Either::Right(dispatch_error.into()))
				},
				_ => {},
			}
		}

		Err(DynamicEventError::UnexpectedChainBehaviour)
	}
}

pub struct EventsDecoder {
	current: OfflineClientAtBlock<StateChainConfig>,
	static_metadata: ArcMetadata,
}

impl EventsDecoder {
	pub fn new(opaque_metadata: &sp_core::OpaqueMetadata) -> Self {
		let current_metadata = subxt::Metadata::decode(&mut opaque_metadata.as_ref())
			.expect("Runtime metadata should be valid.");
		let current = OfflineClient::new_with_config(StateChainConfig::with_fixed_metadata(
			Arc::new(current_metadata),
		))
		.at_block(0u64)
		.expect("Fixed metadata config always resolves a block.");

		// Get the compile time static metadata
		let static_opaque_metadata = state_chain_runtime::Runtime::metadata_at_version(15)
			.expect("Version 15 should be supported by the runtime.");
		let static_metadata = subxt::Metadata::decode(&mut static_opaque_metadata.as_ref())
			.expect("Runtime metadata should be valid.");

		Self { current, static_metadata: Arc::new(static_metadata) }
	}

	pub fn decode_extrinsic_events(
		&self,
		extrinsic_index: usize,
		bytes: Option<Vec<u8>>,
	) -> Result<DynamicEvents, DynamicEventError> {
		let Some(events_bytes) = bytes else {
			return Ok(DynamicEvents { events: vec![] });
		};

		let evs = self.current.events().from_bytes(events_bytes);

		let mut events = vec![];

		for event in evs.iter() {
			let event = event?;

			if event.phase() == subxt::events::Phase::ApplyExtrinsic(extrinsic_index as u32) {
				events.push(DynamicEvent {
					pallet_name: event.pallet_name().to_owned(),
					event_name: event.event_name().to_owned(),
					pallet_index: event.pallet_index(),
					event_index: event.event_index(),
					field_bytes: event.field_bytes().to_vec(),
					current_metadata: self.current.metadata(),
					static_metadata: self.static_metadata.clone(),
				});
			}
		}

		Ok(DynamicEvents { events })
	}
}
