use std::{collections::BTreeMap, fmt::Display, path::PathBuf};

use crate::typediff::{DiscreteMorphism, Point, TypeName, TypeRepr};
use std::fs;

pub struct VirtualFile {
	// condition: all typereprs should be flat!
	definitions: BTreeMap<TypeName, TypeRepr<Point>>,

	path: PathBuf,
}

impl VirtualFile {
	pub fn new(path: PathBuf) -> Self {
		Self { definitions: Default::default(), path }
	}

	pub fn write(&mut self, def: TypeRepr<Point>) -> TypeRepr<Point> {
		use TypeRepr::*;
		match &def {
			a @ Struct { typename, .. } |
			a @ Enum { typename, .. } |
			a @ Tuple { typename, .. } |
			a @ Sequence { typename, .. } => {
				let typename = typename.clone().get_old();
				if !self.definitions.contains_key(&typename) {
					self.definitions.insert(typename.clone(), a.clone());
				}
				return TypeRepr::TypeByName(DiscreteMorphism::Same(typename))
			},
			a @ Primitive(_) => a.clone(),
			a @ TypeByName(discrete_morphism) => a.clone(),
			a @ NotImplemented => todo!(),
		}
	}

	pub fn apply(&self) {
		// create directories
		fs::create_dir_all(self.path.parent().unwrap()).unwrap();

		// create file
		let file = fs::File::create(self.path.clone()).unwrap();

		for repr in &self.definitions {
			println!("writing {repr:?}");
		}
	}
}

impl Display for TypeRepr<Point> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			TypeRepr::Struct { typename, fields } => write!(f, "pub struct"),
			TypeRepr::Enum { typename, variants } => todo!(),
			TypeRepr::Tuple { typename, fields } => todo!(),
			TypeRepr::Sequence { typename, inner } => todo!(),
			TypeRepr::NotImplemented => todo!(),
			TypeRepr::Primitive(_) => todo!(),
			TypeRepr::TypeByName(discrete_morphism) => todo!(),
		}
	}
}
