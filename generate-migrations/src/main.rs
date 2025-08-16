#![feature(os_str_display)]
#![feature(trait_alias)]
#![feature(btree_extract_if)]
#![feature(never_type)]
#![feature(iter_intersperse)]
#![feature(path_add_extension)]

mod diff;
mod metadata;
mod registry;
mod typediff;
mod types;
mod virtual_file;
mod write_migration;

use crate::{
	metadata::get_local_metadata,
	typediff::{MetadataConfig, PalletRef, compare_metadata},
	types::from_metadata::extract_type,
	virtual_file::{Module, VirtualFile},
	write_migration::{FullMigration, PalletMigration},
};

use clap::{Parser, Subcommand};
use std::{collections::BTreeMap, env, path::PathBuf};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
	// /// Optional name to operate on
	// name: Option<String>,

	// /// Sets a custom config file
	// #[arg(short, long, value_name = "FILE")]
	// config: Option<PathBuf>,

	// /// Turn debugging information on
	// #[arg(short, long, action = clap::ArgAction::Count)]
	// debug: u8,

	// #[arg(short, long, value_name = "INCLUDE")]
	// include: Vec<String>,
	#[clap(flatten)]
	config: MetadataConfig,

	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
enum Commands {
	/// does testing things
	Test {
		/// lists test values
		#[arg(short, long)]
		list: bool,
	},

	Check,

	Generate,
}

#[tokio::main]
async fn main() {
	let cli = Cli::parse();

	// You can check for the existence of subcommands, and if found use their
	// matches just as you would the top level cmd
	match &cli.command {
		Commands::Test { list } =>
			if *list {
				println!("Printing testing lists...");
			} else {
				println!("Not printing testing lists...");
			},
		Commands::Check => println!("checking..."),
		Commands::Generate => {
			println!("generating...");
		},
	}

	let current_metadata = get_local_metadata();
	let pallet = current_metadata.pallet_by_name("BitcoinElections").unwrap();

	for item in pallet.storage().unwrap().entries() {
		use subxt::metadata::types::StorageEntryType;
		match item.entry_type() {
			StorageEntryType::Plain(_) => (),
			StorageEntryType::Map { hashers, key_ty, value_ty } => {
				let ty = extract_type(&current_metadata, *value_ty);
				println!("type of val of {} is: {:?}", item.name().to_string(), ty);
			},
		}
	}

	/*

	let result = compare_metadata(&cli.config).await;

	let mut pallet_migrations = BTreeMap::<PalletRef, PalletMigration>::new();
	for (pallet, old_defs) in result.old_definitions {
		// derive file for pallet
		let path = env::current_dir().unwrap().join("gentemp").join(pallet.name.clone());

		pallet_migrations
			.entry(pallet.name)
			.or_insert(PalletMigration {
				old_definitions: Default::default(),
				crate_location: path,
			})
			.old_definitions
			.insert(pallet.chain_instance, old_defs);
	}

	let migration = FullMigration { pallet_migrations: pallet_migrations.into_values().collect() };

	let modules = migration.apply();

	let virtual_files: Vec<VirtualFile> =
		modules.iter().flat_map(|m| m.apply().into_iter()).collect();

	for file in virtual_files {
		file.apply();
	}
	*/
}
