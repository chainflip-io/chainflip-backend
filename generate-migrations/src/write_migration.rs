// struct
// pub fn write()

use std::{collections::BTreeMap, path::PathBuf};

use crate::{
	typediff::{Morphism, Point, TypeRepr},
	virtual_file::VirtualFile,
};

#[derive(Clone)]
pub struct PalletMigration {
	pub old_definitions: Vec<TypeRepr<Point>>,
	pub crate_location: PathBuf,
}

#[derive(Clone)]
pub struct FullMigration {
	pub pallet_migrations: Vec<PalletMigration>,
}

impl PalletMigration {
	pub fn apply(&self) -> Vec<VirtualFile> {
		// migration location
		let path = self.crate_location.join("src").join("generated").join("migration.rs");

		// create file for migration
		let mut file = VirtualFile::new(path);

		// write all definitions
		for def in &self.old_definitions {
			file.write(def.clone());
		}

		vec![file]
	}
}

impl FullMigration {
	pub fn apply(&self) -> Vec<VirtualFile> {
		self.pallet_migrations
			.clone()
			.into_iter()
			.flat_map(|pallet| pallet.apply().into_iter())
			.collect()
	}
}
