use std::{
	collections::BTreeMap,
	sync::{Arc, Mutex},
};

use crate::{to_rpc_error, CustomRpc};

use cf_utilities::dynamic_events::{DynamicEventRecord, EventDecoder};
use codec::Decode;
use frame_metadata::{v15, RuntimeMetadataPrefixed};
use jsonrpsee::{core::RpcResult, proc_macros::rpc, types::error::CallError};
use sc_client_api::HeaderBackend;
use sp_api::{CallApiAt, Core, Metadata};
use sp_runtime::traits::Block as BlockT;

/// This valid across all Substrate chains that use the System pallet.
const SYSTEM_EVENTS_STORAGE_KEY: [u8; 32] =
	hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef780d41e5e16056765bc8461851072c9d7");

#[rpc(server, client, namespace = "cf_dynamic")]
/// API for querying the State Chain dynamicaly.
pub trait DynamicApi {
	/// Get dynamically-encoded events for the provided block hash or the latest block. Optionally
	/// filter the events by the provided JSON pointer paths.
	#[method(name = "events")]
	fn cf_dynamic_events(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<DynamicEventRecord>>;
}

pub type EventDecoderCache = Mutex<BTreeMap<u32, Arc<EventDecoder>>>;

impl<C, B> CustomRpc<C, B>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	C: sp_api::ProvideRuntimeApi<B> + Send + Sync + 'static,
	C::Api: sp_api::Metadata<B> + sp_api::Core<B>,
{
	fn event_decoder_at_hash(
		&self,
		hash: state_chain_runtime::Hash,
	) -> RpcResult<Arc<EventDecoder>> {
		let runtime = self.client.runtime_api();
		let runtime_version = runtime.version(hash).map_err(to_rpc_error)?.spec_version;

		log::debug!("getting EventDecoder for runtime_version: {:?}", runtime_version);

		let mut cache = self.type_metadata.lock().unwrap();

		Ok(if let Some(decoder) = cache.get(&runtime_version).cloned() {
			log::debug!("event_decoder_at_hash fetched from cache");
			decoder.clone()
		} else {
			log::debug!("EventDecoder not in cache, fetching metadata...");
			let (types, outer_enums) = match RuntimeMetadataPrefixed::decode(
				&mut runtime
					.metadata_at_version(hash, 15)
					.map_err(to_rpc_error)?
					.expect("V15 Metadata must be supported by the runtime.")
					.as_slice(),
			)
			.map_err(to_rpc_error)?
			{
				RuntimeMetadataPrefixed(
					_,
					frame_metadata::RuntimeMetadata::V15(v15::RuntimeMetadataV15 {
						types,
						outer_enums,
						..
					}),
				) => Ok::<_, CallError>((types, outer_enums)),
				RuntimeMetadataPrefixed(_, other) => Err(CallError::Failed(anyhow::anyhow!(
					"Unsupported metadata version {}.",
					other.version()
				))),
			}?;

			log::debug!("Metadata retrieved, creating new EventDecoder...");

			let decoder = Arc::new(EventDecoder::new(types, outer_enums.error_enum_ty.id));
			assert!(
				cache.insert(runtime_version, decoder.clone(),).is_none(),
				"Tried to insert a duplicate event decoder at runtime version {}",
				runtime_version
			);

			log::debug!("New EventDecoder created and cached.");

			decoder
		})
	}
}

impl<C, B> DynamicApiServer for CustomRpc<C, B>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	C: sp_api::ProvideRuntimeApi<B> + Send + Sync + 'static + HeaderBackend<B> + CallApiAt<B>,
	C::Api: sp_api::Metadata<B> + sp_api::Core<B>,
{
	fn cf_dynamic_events(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<DynamicEventRecord>> {
		let hash = self.unwrap_or_best(at);

		log::debug!("cf_dynamic_events for hash: {}", hash);

		let events_data = self.storage_query_api().with_state_backend(hash, || {
			frame_support::storage::unhashed::get(&SYSTEM_EVENTS_STORAGE_KEY).unwrap_or_default()
		})?;

		log::debug!("cf_dynamic_events raw event data: 0x{}", hex::encode(&events_data));

		self.event_decoder_at_hash(hash)
			.map_err(to_rpc_error)?
			.decode_events(events_data)
			.map_err(to_rpc_error)
	}
}
