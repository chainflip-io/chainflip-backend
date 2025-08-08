use codec::{Decode, Encode};
use frame_metadata::{RuntimeMetadata, v14::RuntimeMetadataV14};
use scale_info::{Field, MetaType, TypeDefPrimitive, form::PortableForm};
use std::{
	collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
	env::{self, var},
	fmt::Debug,
	fs,
	path::{Path, PathBuf, absolute},
	process,
	str::FromStr,
};
use subxt::metadata::types::StorageEntryType;

pub fn get_local_metadata() -> subxt::Metadata {
	let metadata = state_chain_runtime::Runtime::metadata().1;
	let encoded = state_chain_runtime::Runtime::metadata().encode();
	let new_metadata = <subxt::Metadata as Decode>::decode(&mut &*encoded).unwrap();
	new_metadata
}

pub async fn get_mainnet_metadata() -> subxt::Metadata {
	let subxt_client = subxt::OnlineClient::<subxt::PolkadotConfig>::from_url(
		"wss://mainnet-archive.chainflip.io",
	)
	.await
	.unwrap();
	subxt_client.metadata()
}
