use crate::{cf_static_runtime, subxt_state_chain_config::StateChainConfig};
use codec::Decode;
use frame_support::dispatch::DispatchInfo;
use sp_runtime::{DispatchError, Either};
use subxt::{events::StaticEvent, ext::subxt_core};

pub type DynamicEvent = subxt::events::EventDetails<StateChainConfig>;

#[derive(thiserror::Error, Debug)]
pub enum DynamicEventError {
	#[error(transparent)]
	SubxtError(#[from] subxt_core::Error),
	#[error("Unexpected chain behaviour, ExtrinsicSuccess or ExtrinsicFailed event not found.")]
	UnexpectedChainBehaviour,
	#[error("Could not decode event, please consider upgrading your node. {0}")]
	EventDecodeError(String),
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

	pub fn find_static_event<E: StaticEvent>(&self) -> Result<Option<E>, DynamicEventError> {
		for event in self.events.iter() {
			if (event.pallet_name(), event.variant_name()) == (E::PALLET, E::EVENT) {
				return match event.as_event::<E>() {
					Ok(maybe_event) => Ok(maybe_event),
					Err(e) => Err(DynamicEventError::EventDecodeError(e.to_string())),
				}
			}
		}
		Ok(None)
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
	subxt_metadata: subxt::Metadata,
}

impl Default for EventsDecoder {
	fn default() -> Self {
		let opaque_metadata = state_chain_runtime::Runtime::metadata_at_version(15)
			.expect("Version 15 should be supported by the runtime.");

		Self::new(opaque_metadata)
	}
}

impl EventsDecoder {
	pub fn new(opaque_metadata: sp_core::OpaqueMetadata) -> Self {
		let metadata =
			frame_metadata::RuntimeMetadataPrefixed::decode(&mut opaque_metadata.as_slice())
				.expect("Runtime metadata should be valid.");

		// Ok(OfflineClient::<StateChainConfig>::new(
		//     genesis_hash,
		//     subxt::client::RuntimeVersion {
		//         spec_version: version.spec_version,
		//         transaction_version: version.transaction_version,
		//     },
		//     subxt::Metadata::try_from(metadata).map_err(internal_error)?,
		// ))

		let subxt_metadata =
			subxt::Metadata::try_from(metadata).expect("Metadata should be valid.");

		Self { subxt_metadata }
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
			self.subxt_metadata.clone(),
		);

		let mut events = vec![];

		for event in evs.iter() {
			let event_details = event?;

			if event_details.phase() == subxt::events::Phase::ApplyExtrinsic(extrinsic_index as u32)
			{
				events.push(event_details);
			}
		}

		Ok(DynamicEvents { events })
	}
}
//
// fn convert_dispatch_error(error: DispatchError) -> sp_runtime::DispatchError {
// 	match error {
// 		DispatchError::Other => sp_runtime::DispatchError::Other("Other error"),
// 		DispatchError::CannotLookup => sp_runtime::DispatchError::CannotLookup,
// 		DispatchError::BadOrigin => sp_runtime::DispatchError::BadOrigin,
// 		DispatchError::Module(e) => {
// 			sp_runtime::DispatchError::Module(e.into())
// 		}
// 		DispatchError::ConsumerRemaining => sp_runtime::DispatchError::ConsumerRemaining,
// 		DispatchError::NoProviders => sp_runtime::DispatchError::NoProviders,
// 		DispatchError::TooManyConsumers => sp_runtime::DispatchError::TooManyConsumers,
// 		DispatchError::Token(e) => sp_runtime::DispatchError::Token(e.into()), // Convert TokenError if
// needed 		DispatchError::Arithmetic(e) => sp_runtime::DispatchError::Arithmetic(e.into()), //
// Convert ArithmeticError if needed 		DispatchError::Transactional(_) =>
// sp_runtime::DispatchError::Transactional(error.into()), // Convert TransactionalError if needed
// 	}
// }

impl From<cf_static_runtime::runtime_types::sp_runtime::DispatchError> for DispatchError {
	fn from(error: cf_static_runtime::runtime_types::sp_runtime::DispatchError) -> Self {
		match error {
			// TODO: investigate why the types are not symmetrical. may be subxt-cli version
			// mismatch
			cf_static_runtime::runtime_types::sp_runtime::DispatchError::Other =>
				sp_runtime::DispatchError::Other("Other error"),

			cf_static_runtime::runtime_types::sp_runtime::DispatchError::CannotLookup =>
				sp_runtime::DispatchError::CannotLookup,

			cf_static_runtime::runtime_types::sp_runtime::DispatchError::BadOrigin =>
				sp_runtime::DispatchError::BadOrigin,

			cf_static_runtime::runtime_types::sp_runtime::DispatchError::Module(module_error) =>
				sp_runtime::DispatchError::Module(sp_runtime::ModuleError {
					index: module_error.index,
					error: module_error.error,
					message: None,
				}),

			cf_static_runtime::runtime_types::sp_runtime::DispatchError::ConsumerRemaining =>
				sp_runtime::DispatchError::ConsumerRemaining,

			cf_static_runtime::runtime_types::sp_runtime::DispatchError::NoProviders =>
				sp_runtime::DispatchError::NoProviders,

			cf_static_runtime::runtime_types::sp_runtime::DispatchError::Token(token_error) =>
				sp_runtime::DispatchError::Token(match token_error {
					cf_static_runtime::runtime_types::sp_runtime::TokenError::FundsUnavailable =>
						sp_runtime::TokenError::FundsUnavailable,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::OnlyProvider =>
						sp_runtime::TokenError::OnlyProvider,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::BelowMinimum =>
						sp_runtime::TokenError::BelowMinimum,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::CannotCreate =>
						sp_runtime::TokenError::CannotCreate,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::UnknownAsset =>
						sp_runtime::TokenError::UnknownAsset,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::Frozen =>
						sp_runtime::TokenError::Frozen,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::Unsupported =>
						sp_runtime::TokenError::Unsupported,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::CannotCreateHold =>
						sp_runtime::TokenError::CannotCreateHold,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::NotExpendable =>
						sp_runtime::TokenError::NotExpendable,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::Blocked =>
						sp_runtime::TokenError::Blocked,
				}),

			_ => sp_runtime::DispatchError::Other("Unknown error"),
		}
	}
}

impl From<cf_static_runtime::runtime_types::frame_support::dispatch::DispatchInfo>
	for frame_support::dispatch::DispatchInfo
{
	fn from(info: cf_static_runtime::runtime_types::frame_support::dispatch::DispatchInfo) -> Self {
		Self {
			weight: frame_support::weights::Weight::from_parts(info.weight.ref_time, info.weight.proof_size),
			class: match info.class {
				cf_static_runtime::runtime_types::frame_support::dispatch::DispatchClass::Normal =>
					frame_support::dispatch::DispatchClass::Normal,
				cf_static_runtime::runtime_types::frame_support::dispatch::DispatchClass::Operational =>
					frame_support::dispatch::DispatchClass::Operational,
				cf_static_runtime::runtime_types::frame_support::dispatch::DispatchClass::Mandatory =>
					frame_support::dispatch::DispatchClass::Mandatory,
			},
			pays_fee: match info.pays_fee {
				cf_static_runtime::runtime_types::frame_support::dispatch::Pays::Yes =>
					frame_support::dispatch::Pays::Yes,
				cf_static_runtime::runtime_types::frame_support::dispatch::Pays::No =>
					frame_support::dispatch::Pays::No,
			},
		}
	}
}
