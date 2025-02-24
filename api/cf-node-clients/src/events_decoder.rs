use crate::{cf_static_runtime, subxt_state_chain_config::StateChainConfig};
use codec::Decode;
use frame_support::dispatch::DispatchInfo;
use sp_runtime::{DispatchError, Either};
use subxt::{
	events::StaticEvent,
	ext::{scale_decode, subxt_core, subxt_core::events::EventMetadataDetails},
};

#[derive(thiserror::Error, Debug)]
pub enum DynamicEventError {
	#[error(transparent)]
	SubxtError(#[from] subxt_core::Error),
	#[error("Unexpected chain behaviour, ExtrinsicSuccess or ExtrinsicFailed event not found.")]
	UnexpectedChainBehaviour,
	#[error("Could not decode event, it might be because you running an old binary please consider upgrading. {0}")]
	EventDecodeError(String),
	#[error("Event unknown to static metadata, it might be because you running an old binary please consider upgrading your node")]
	EventUnknownToStaticMetadata,
	#[error("{0} event was not found")]
	StaticEventNotFound(&'static str),
}

#[derive(Debug, Clone)]
pub struct DynamicEvent {
	event: subxt::events::EventDetails<StateChainConfig>,
	static_metadata: subxt::Metadata,
}

impl DynamicEvent {
	pub fn event_static_metadata<E: StaticEvent>(
		&self,
	) -> Result<EventMetadataDetails, DynamicEventError> {
		// Make sure the event is still known to the static metadata. i.e. it was not removed in a
		// newer runtime version
		if self.event.variant_name() != E::EVENT {
			return Err(DynamicEventError::EventUnknownToStaticMetadata);
		}
		let pallet_metadata = self
			.static_metadata
			.pallet_by_name(E::PALLET)
			.ok_or_else(|| DynamicEventError::EventUnknownToStaticMetadata)?;

		let variant_metadata = pallet_metadata
			.event_variant_by_index(self.event.variant_index())
			.ok_or_else(|| DynamicEventError::EventUnknownToStaticMetadata)?;

		Ok(EventMetadataDetails { pallet: pallet_metadata, variant: variant_metadata })
	}

	pub fn as_event<E: StaticEvent>(&self) -> Result<Option<E>, DynamicEventError> {
		self.event.as_event::<E>().map_err(DynamicEventError::from)
	}

	pub fn as_event_strict<E: StaticEvent>(&self) -> Result<Option<E>, DynamicEventError> {
		let ev_metadata = self.event.event_metadata();
		if ev_metadata.pallet.name() == E::PALLET && ev_metadata.variant.name == E::EVENT {
			let ev_static_metadata = self.event_static_metadata::<E>()?;

			// Decode the fields using the old static metadata
			let mut bytes = self.event.field_bytes(); // Get the event's field bytes
			let mut static_fields = ev_static_metadata
				.variant
				.fields
				.iter()
				.map(|f| scale_decode::Field::new(f.ty.id, f.name.as_deref()));

			let decoded =
				E::decode_as_fields(&mut bytes, &mut static_fields, self.static_metadata.types())
					.map_err(subxt_core::Error::from)?;

			// **STRICT NAMES CHECK **: Ensure decoded fields match old static metadata fields
			let static_field_names: Vec<_> =
				ev_static_metadata.variant.fields.iter().map(|f| f.name.as_deref()).collect();

			let actual_field_names: Vec<_> =
				ev_metadata.variant.fields.iter().map(|f| f.name.as_deref()).collect();

			if static_field_names != actual_field_names {
				return Err(DynamicEventError::EventDecodeError(format!(
					"{} event strict fields check failed: expected {:?}, got {:?}",
					E::EVENT,
					static_field_names,
					actual_field_names
				)));
			}

			// **STRICT BYTE CHECK**: Ensure no extra bytes remain
			if !bytes.is_empty() {
				return Err(DynamicEventError::EventDecodeError(format!(
					"{} event strict byte check failed",
					E::EVENT
				)));
			}

			Ok(Some(decoded))
		} else {
			Ok(None)
		}
	}

	pub fn pallet_name(&self) -> &str {
		self.event.pallet_name()
	}

	pub fn variant_name(&self) -> &str {
		self.event.variant_name()
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
	pub fn find_event(&self, pallet_name: &str, event_name: &str) -> Option<&DynamicEvent> {
		self.events
			.iter()
			.find(|event| event.pallet_name() == pallet_name && event.variant_name() == event_name)
	}

	pub fn find_static_event<E: StaticEvent>(
		&self,
		is_strict: bool,
	) -> Result<Option<E>, DynamicEventError> {
		for event in self.events.iter() {
			if (event.pallet_name(), event.variant_name()) == (E::PALLET, E::EVENT) {
				return if is_strict { event.as_event_strict::<E>() } else { event.as_event::<E>() }
			}
		}
		Ok(None)
	}

	pub fn find_all_static_events<E: StaticEvent>(
		&self,
		is_strict: bool,
	) -> Result<Vec<E>, DynamicEventError> {
		self.events
			.iter()
			.filter(|event| (event.pallet_name(), event.variant_name()) == (E::PALLET, E::EVENT))
			.filter_map(|event| {
				if is_strict {
					event.as_event_strict::<E>().transpose()
				} else {
					event.as_event::<E>().transpose()
				}
			})
			.collect()
	}

	pub fn extrinsic_result(
		&self,
	) -> Result<Either<DispatchInfo, DispatchError>, DynamicEventError> {
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
	current_metadata: subxt::Metadata,
	static_metadata: subxt::Metadata,
}

impl EventsDecoder {
	pub fn new(opaque_metadata: &sp_core::OpaqueMetadata) -> Self {
		let current_metadata = subxt::Metadata::decode(&mut opaque_metadata.as_ref())
			.expect("Runtime metadata should be valid.");

		// Get the compile time static metadata
		let static_opaque_metadata = state_chain_runtime::Runtime::metadata_at_version(15)
			.expect("Version 15 should be supported by the runtime.");
		let static_metadata = subxt::Metadata::decode(&mut static_opaque_metadata.as_ref())
			.expect("Runtime metadata should be valid.");

		Self { current_metadata, static_metadata }
	}

	pub fn decode_extrinsic_events(
		&self,
		extrinsic_index: usize,
		bytes: Option<Vec<u8>>,
	) -> Result<DynamicEvents, DynamicEventError> {
		let Some(events_bytes) = bytes else {
			return Ok(DynamicEvents { events: vec![] });
		};

		let evs = subxt::events::Events::<StateChainConfig>::decode_from(
			events_bytes,
			self.current_metadata.clone(),
		);

		let mut events = vec![];

		for event in evs.iter() {
			let event_details = event?;

			if event_details.phase() == subxt::events::Phase::ApplyExtrinsic(extrinsic_index as u32)
			{
				events.push(DynamicEvent {
					event: event_details,
					static_metadata: self.static_metadata.clone(),
				});
			}
		}

		Ok(DynamicEvents { events })
	}
}
