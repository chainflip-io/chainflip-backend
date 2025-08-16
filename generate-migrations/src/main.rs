#![feature(os_str_display)]
#![feature(trait_alias)]
#![feature(btree_extract_if)]
#![feature(never_type)]
#![feature(iter_intersperse)]
#![feature(path_add_extension)]

mod diff;
mod typediff;
mod virtual_file;
mod write_migration;
// mod types;
mod from_metadata;
mod registry;
mod types2;

use crate::{
	typediff::{MetadataConfig, PalletRef, compare_metadata},
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
}
