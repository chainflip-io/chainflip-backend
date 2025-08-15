// struct
// pub fn write()

use std::{collections::BTreeMap, env, path::PathBuf};

use crate::{
	typediff::{Morphism, PalletInstanceContent, Point, TypeRepr},
	virtual_file::Module,
};

#[derive(Clone)]
pub struct PalletMigration {
	/// different instantiations of this pallet have (possibly) different definitions of types
	pub old_definitions: BTreeMap<Option<String>, PalletInstanceContent>,
	pub crate_location: PathBuf,
}

#[derive(Clone)]
pub struct FullMigration {
	pub pallet_migrations: Vec<PalletMigration>,
}

// impl PalletMigration {
// 	pub fn apply(&self) -> Vec<Module> {
// 		// migration location
// 		let path = env::current_dir().unwrap().join("gentemp").join("old")
// 		//self.crate_location.join("src").join("generated").join("migration.rs");

// 		// create file for migration
// 		let mut file = Module::new(path);

// 		// write all definitions
// 		for (_chain, defs) in &self.old_definitions {
// 			for def in defs {
// 				file.write(def.clone());
// 			}
// 		}

// 		vec![file]
// 	}
// }

impl FullMigration {
	pub fn apply(&self) -> Vec<Module> {
		// create module for old type defs
		let path = env::current_dir().unwrap().join("gentemp").join("old");
		let mut module = Module::new(path);

		for defs in self.pallet_migrations.clone().into_iter().map(|pallet| pallet.old_definitions)
		{
			for (_chain, defs) in defs {
				for def in defs {
					module.write(def);
				}
			}
		}

		vec![module]
	}
}
