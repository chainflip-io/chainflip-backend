#![feature(os_str_display)]
#![feature(btree_extract_if)]

mod diff;

use crate::diff::NodeDiff;
use codec::{Decode, Encode};
use frame_metadata::{RuntimeMetadata, v14::RuntimeMetadataV14};
use scale_info::form::PortableForm;
use std::{
	collections::{BTreeMap, HashSet},
	env, fs,
	path::{Path, PathBuf, absolute},
	process,
	str::FromStr,
};
use subxt::metadata::types::StorageEntryType;
use walkdir::WalkDir;

fn generate_type_packages(root_path: PathBuf, target_path: PathBuf) -> anyhow::Result<()> {
	// find all subdirs of migrate_to which contain a `Cargo.toml`

	for entry in WalkDir::new(root_path.clone())
		.into_iter()
		.filter_entry(|entry| {
			!entry.clone().into_path().display().to_string().starts_with("/.") &&
				!entry.clone().into_path().display().to_string().contains("target")
		})
		.filter_map(|e| e.ok())
		.filter(|e| e.metadata().unwrap().is_file())
	{
		let path = entry.path();

		if format!("{}", path.file_name().unwrap().display()) == "Cargo.toml".to_string() {
			//--- setup paths ----
			// the source paths
			let package_path = path.parent().unwrap();
			let relative_package_path = package_path.strip_prefix(root_path.clone()).unwrap();

			// the target paths
			let mut relative_package_path_target = target_path.clone();
			relative_package_path_target.extend(relative_package_path);
			fs::create_dir_all(&relative_package_path_target).unwrap();

			println!(
				"package: {} => target_package: {}",
				relative_package_path.display(),
				relative_package_path_target.display()
			);

			//--- copy files ---
			// move the cargo toml
			let from = root_path.join(relative_package_path).join("Cargo.toml");
			let to = relative_package_path_target.join("Cargo.toml");
			print!(" |> copying Cargo.toml... ");
			fs::copy(from, to).unwrap();
			println!("Done.");

			if relative_package_path.display().to_string().contains("state-chain") {
				// macroexpand crate and move lib
				print!(" |> Macro expanding lib file... ");
				let to = relative_package_path_target.join("src/lib.rs");
				fs::create_dir_all(to.parent().unwrap()).unwrap();
				let macro_expanded_lib_file = fs::File::create(to).unwrap();
				let mut command = process::Command::new("cargo")
					.current_dir(root_path.join(relative_package_path))
					.args(["expand", "--lib"])
					.stdout(macro_expanded_lib_file)
					.spawn()
					.unwrap();
				command.wait().unwrap();
			}
		}
	}
	Ok(())
}

fn main2() {
	let temp_directory =
		env::var("TEMP_MIGRATIONS_DIR").expect("TEMP_MIGRATIONS_DIR env var has to be set");
	let temp_directory = PathBuf::from_str(&temp_directory).unwrap();
	fs::create_dir_all(&temp_directory).unwrap();

	let migrate_to = env::var("MIGRATE_TO").expect("MIGRATE_TO env var has to be set");
	let migrate_to = Path::canonicalize(absolute(migrate_to).unwrap().as_path()).unwrap();

	generate_type_packages(migrate_to, temp_directory).unwrap();

	println!("Hello, world!");
}

pub fn diff_metadata(metadata: RuntimeMetadataV14) {}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
struct StorageLocation {
	pallet: String,
	storage_name: String,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
enum PortableStorageEntryType {
	Plain(scale_info::Type<PortableForm>),
	Map(scale_info::Type<PortableForm>, scale_info::Type<PortableForm>),
}

pub fn get_all_storage_entries(
	metadata: &subxt::Metadata,
) -> BTreeMap<StorageLocation, PortableStorageEntryType> {
	metadata
		.pallets()
		.flat_map(|pallet| {
			pallet.storage().unwrap().entries().iter().cloned().map(move |entry| {
				(
					StorageLocation {
						pallet: pallet.name().to_string(),
						storage_name: entry.name().to_string(),
					},
					match entry.entry_type() {
						StorageEntryType::Plain(ty) => PortableStorageEntryType::Plain(
							metadata.types().resolve(*ty).unwrap().clone(),
						),
						StorageEntryType::Map { hashers, key_ty, value_ty } =>
							PortableStorageEntryType::Map(
								metadata.types().resolve(*key_ty).unwrap().clone(),
								metadata.types().resolve(*value_ty).unwrap().clone(),
							),
					},
				)
			})
		})
		.collect()
}

#[tokio::main]
async fn main() {
	let metadata = state_chain_runtime::Runtime::metadata().1;
	let pallets: Vec<_> = match metadata {
		RuntimeMetadata::V14(runtime_metadata_v14) =>
			runtime_metadata_v14.pallets.into_iter().map(|pallet| pallet.name).collect(),
		// RuntimeMetadata::V15(runtime_metadata_v15) => todo!(),
		_ => panic!("wrong metadata version!"),
	};

	let encoded = state_chain_runtime::Runtime::metadata().encode();
	let new_metadata = <subxt::Metadata as Decode>::decode(&mut &*encoded).unwrap();

	let get_pallet_names = |metadata: subxt::Metadata| {
		metadata
			.clone()
			.pallets()
			.map(|pallet| pallet.name().to_string())
			.collect::<Vec<_>>()
	};

	let subxt_client = subxt::OnlineClient::<subxt::PolkadotConfig>::from_url(
		"wss://mainnet-archive.chainflip.io",
	)
	.await
	.unwrap();
	let old_metadata = subxt_client.metadata();

	println!("online pallets: {:?}", get_pallet_names(old_metadata.clone()));
	println!("local  pallets: {:?}", get_pallet_names(new_metadata.clone()));

	// compute storage objects that differ
	let old_storage = get_all_storage_entries(&old_metadata);
	let new_storage = get_all_storage_entries(&new_metadata);
	let mut diff = diff::diff(old_storage, new_storage);
	diff.retain(|_key, value| match value {
		NodeDiff::Both(v, w) if v == w => false,
		_ => true,
	});

	for (location, entry) in diff {
		print!("{}::{}: ", location.pallet, location.storage_name);
		match entry {
			NodeDiff::Left(_) => println!("DELETED"),
			NodeDiff::Right(_) => println!("CREATED"),
			NodeDiff::Both(_, _) => println!("MODIFIED"),
		}
	}
}
