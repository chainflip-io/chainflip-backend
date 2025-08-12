#![feature(os_str_display)]
#![feature(trait_alias)]
#![feature(btree_extract_if)]
#![feature(never_type)]

mod diff;
mod write_migration;
// mod container;

use crate::diff::{NodeDiff, diff};
use codec::{Decode, Encode};
use derive_where::derive_where;
use frame_metadata::{RuntimeMetadata, v14::RuntimeMetadataV14};
use scale_info::{Field, MetaType, TypeDefPrimitive, form::PortableForm};
use std::{
	collections::{BTreeMap, BTreeSet, HashSet},
	env::{self, var},
	fmt::Debug,
	fs,
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

type FlatType = Vec<TypePath>;

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
enum PortableStorageEntryType<Ty> {
	Plain(Ty),
	Map(Ty, Ty),
}

pub fn get_all_storage_entries(
	metadata: &subxt::Metadata,
) -> BTreeMap<StorageLocation, PortableStorageEntryType<u32>> {
	metadata
		.pallets()
		.filter(|pallet| pallet.name().contains("Ingress"))
		.flat_map(|pallet| {
			pallet.storage().unwrap().entries().iter().cloned().map(move |entry| {
				(
					StorageLocation {
						pallet: pallet.name().to_string(),
						storage_name: entry.name().to_string(),
					},
					match entry.entry_type() {
						StorageEntryType::Plain(ty) => PortableStorageEntryType::Plain(*ty),
						StorageEntryType::Map { hashers, key_ty, value_ty } =>
							PortableStorageEntryType::Map(*key_ty, *value_ty),
					},
				)
			})
		})
		.collect()
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
enum PathComponent {
	Field { index: usize, name: String },
	Variant { index: usize, name: String },
	Primitive(TypeDefPrimitive),
}
type TypePath = Vec<PathComponent>;

pub fn type_into_path_components(metadata: &subxt::Metadata, ty: u32) -> FlatType {
	use scale_info::TypeDef::*;
	match metadata.types().resolve(ty).unwrap().clone().type_def {
		Composite(type_def_composite) => type_def_composite
			.fields
			.into_iter()
			.enumerate()
			.flat_map(|(index, field)| {
				let inner_paths = type_into_path_components(metadata, field.ty.id);
				inner_paths.into_iter().map(move |mut inner_path| {
					inner_path.insert(0, PathComponent::Field {
						name: field.name.clone().unwrap_or("".to_string()),
						index: 0,
					});
					inner_path
				})
			})
			.collect(),
		Variant(type_def_variant) => type_def_variant
			.variants
			.into_iter()
			.flat_map(|variant| {
				variant.fields.clone().into_iter().enumerate().flat_map(
					move |(field_index, field)| {
						// println!("looking at: variant    {:?}", variant.name.clone() );
						// println!("looking at: enum field {:?}",
						// field.name.clone().unwrap_or("".to_string()) );

						let variant = variant.clone();
						let inner_paths = type_into_path_components(metadata, field.ty.id);
						inner_paths.into_iter().map(move |mut inner_path| {
							inner_path.insert(0, PathComponent::Field {
								name: field.name.clone().unwrap_or("".to_string()),
								index: 0,
							});
							inner_path.insert(0, PathComponent::Variant {
								name: variant.name.clone(),
								index: variant.index as usize,
							});
							inner_path
						})
					},
				)
			})
			.collect(),
		Sequence(type_def_sequence) => vec![],
		Array(type_def_array) => vec![],
		Tuple(type_def_tuple) => vec![],
		Primitive(type_def_primitive) => vec![vec![PathComponent::Primitive(type_def_primitive)]],
		Compact(type_def_compact) => vec![],
		BitSequence(type_def_bit_sequence) => vec![],
	}
}

type Diff<A> = NodeDiff<A, A>;

#[derive(Clone, PartialEq, Debug)]
enum CompactDiff<Point, Morphism> {
	Removed(Point),
	Added(Point),
	Change(Point, Point),
	Inherited(Morphism),
	Unchanged(Point),
}

impl<Point: PartialEq + Clone, Morphism: GetIdentity<Point = Point>> CompactDiff<Point, Morphism> {
	fn compact_inherited(m: Morphism) -> Self {
		m.try_get_identity()
			.map(CompactDiff::Unchanged)
			.unwrap_or(CompactDiff::Inherited(m))
	}
}

trait GetIdentity {
	type Point;
	fn try_get_identity(&self) -> Option<Self::Point>;
}

impl<Point: PartialEq + Clone, Morphism: GetIdentity<Point = Point>> GetIdentity
	for CompactDiff<Point, Morphism>
{
	type Point = Point;
	fn try_get_identity(&self) -> Option<Point> {
		match self {
			CompactDiff::Removed(_) => None,
			CompactDiff::Added(_) => None,
			CompactDiff::Change(a, b) =>
				if a == b {
					Some(a.clone())
				} else {
					None
				},
			CompactDiff::Inherited(x) => x.try_get_identity(),
			CompactDiff::Unchanged(a) => Some(a.clone()),
		}
	}
}

impl GetIdentity for StructField<Morphism> {
	type Point = StructField<Point>;

	fn try_get_identity(&self) -> Option<StructField<Point>> {
		let StructField { name, position, ty } = self;
		Some(StructField {
			name: name.clone(),
			position: position.clone(),
			ty: ty.try_get_identity()?,
		})
	}
}

impl GetIdentity for EnumVariant<Morphism> {
	type Point = EnumVariant<Point>;

	fn try_get_identity(&self) -> Option<Self::Point> {
		Some(EnumVariant {
			name: self.name.clone(),
			fields: self
				.fields
				.iter()
				.map(GetIdentity::try_get_identity)
				.collect::<Option<Vec<_>>>()?,
		})
	}
}

// What we want to do is:
//
//  - Convert type
//
// So we have a type structure that in its fields contains either a single value or a morphism.
//
// Then there are two types
trait CommonBounds = Debug + Clone + PartialEq;

trait CellType {
	type Of<Point: CommonBounds, Morphism: CommonBounds>: CommonBounds;
}

#[derive(Debug)]
struct Point;
impl CellType for Point {
	type Of<Point: CommonBounds, Morphism: CommonBounds> = Point;
}

#[derive(Debug)]
struct Morphism;
impl CellType for Morphism {
	type Of<Point: CommonBounds, Morphism: CommonBounds> = CompactDiff<Point, Morphism>;
}

#[derive_where(Clone, PartialEq, Debug;)]
struct StructField<X: CellType> {
	name: String,
	position: usize,
	ty: X::Of<TypeRepr<Point>, TypeRepr<Morphism>>,
}

#[derive_where(Clone, PartialEq, Debug;)]
struct EnumVariant<X: CellType> {
	name: String,
	fields: Vec<X::Of<StructField<Point>, StructField<Morphism>>>,
}

#[derive_where(Clone, Debug, PartialEq;)]
enum TypeRepr<X: CellType> {
	Struct { fields: Vec<X::Of<StructField<Point>, StructField<Morphism>>> },
	Enum { variants: Vec<X::Of<EnumVariant<Point>, EnumVariant<Morphism>>> },
	NotImplemented,
	Primitive(X::Of<TypeDefPrimitive, !>),
	TypeByName,
}

#[derive(Debug, PartialEq, Clone)]
struct Migration {
	type_from: String,
	type_to: String,
	edits: Vec<String>,
	inner_migrations: BTreeMap<String,Migration>
}

impl Migration {
	pub fn from_updates(updates: Vec<Update>) -> Option<Migration> {
		let edits = updates.iter().filter_map(|update| 
			match update {
				Update::Item(a) => Some(a.clone()),
				Update::Inner(migrations) => None,
				Update::None => None,
			}
		).collect::<Vec<_>>();

		let inner_migrations = updates.iter().filter_map(|update|
			match update {
				Update::Item(_) => None,
				Update::Inner(migration) => Some(("".to_string(), migration.clone())),
				Update::None => None,
			}
		).collect::<BTreeMap<_,_>>();

		if edits.len() > 0 || inner_migrations.len() > 0 {
			Some(Migration { type_from: "".to_string(), type_to: "".to_string(), edits, inner_migrations })	
		} else {
			None
		}
	}
}

#[derive(Debug, PartialEq, Clone)]
enum Update {
	Item(String),
	Inner(Migration),
	None
}

impl Update {
	pub fn from_updates(updates: Vec<Update>) -> Update {
		match Migration::from_updates(updates) {
			Some(migration) => Update::Inner(migration),
			None => Update::None,
		}
	}
}

// enum Migration {
// 	Inner {
// 		field: String,
// 		migration: 
// 	}
// }

// impl Migration {
// 	pub fn in_path(self, path_component: String) -> BTreeMap<String,Migration> {
// 		self.inner_migrations.into_iter()
// 			.map(|(key, value)|
// 				(format!("{path_component}::{key}"), value)
// 			)
// 			.chain(self.edits)
// 	}
// }

impl<P: Debug, M: GetUpdated> GetUpdated for CompactDiff<P,M> {
	fn get_updated(&self) -> Update {
		match self {
			CompactDiff::Removed(b) => Update::Item(format!("- {b:?}")),
			CompactDiff::Added(a) => Update::Item(format!("+ {a:?}")),
			CompactDiff::Change(a, b) => Update::Item(format!("C {a:?} => {b:?}")),
			CompactDiff::Inherited(f) => f.get_updated(),
			CompactDiff::Unchanged(_) => Update::None,
		}
	}
}

impl GetUpdated for StructField<Morphism> {
	fn get_updated(&self) -> Update {
		self.ty.get_updated()
	}
}

impl GetUpdated for EnumVariant<Morphism> {
	fn get_updated(&self) -> Update {
		Update::from_updates(self.fields.iter().map(|f| f.get_updated()).collect())
	}
}

impl GetUpdated for TypeRepr<Morphism> {
	fn get_updated(&self) -> Update {
		match self {
			TypeRepr::Struct { fields } => Update::from_updates(fields.iter().map(|field| field.get_updated()).collect()),
			TypeRepr::Enum { variants } => Update::from_updates(variants.iter().map(|field| field.get_updated()).collect()),
			TypeRepr::NotImplemented => Update::None,
			TypeRepr::Primitive(x) => Update::None,
			TypeRepr::TypeByName => Update::None,
		}
	}
}


trait GetUpdated {
	fn get_updated(&self) -> Update;
}

impl GetIdentity for TypeRepr<Morphism> {
	type Point = TypeRepr<Point>;

	fn try_get_identity(&self) -> Option<Self::Point> {
		match self {
			// TypeRepr::Struct { fields } => Some(TypeRepr::Struct {
			// 		fields: fields.iter().map(|field|
			// field.try_get_identity()).collect::<Option<Vec<_>>>()?, 	}),
			// TypeRepr::Enum { variants } => Some(TypeRepr::Enum {
			// 		variants: variants.iter().map(|variant|
			// variant.try_get_identity()).collect::<Option<Vec<_>>>()?, 	}),
			TypeRepr::Struct { fields } =>
				if fields
					.iter()
					.map(|field| field.try_get_identity())
					.collect::<Option<Vec<_>>>()
					.is_some()
				{
					Some(TypeRepr::TypeByName)
				} else {
					None
				},
			TypeRepr::Enum { variants } =>
				if variants
					.iter()
					.map(|field| field.try_get_identity())
					.collect::<Option<Vec<_>>>()
					.is_some()
				{
					Some(TypeRepr::TypeByName)
				} else {
					None
				},
			TypeRepr::NotImplemented => None,
			TypeRepr::Primitive(type_def_primitive) => None,
			TypeRepr::TypeByName => None,
		}
	}
}

pub fn compare_types(
	metadata1: &subxt::Metadata,
	ty1: u32,
	metadata2: &subxt::Metadata,
	ty2: u32,
) -> CompactDiff<TypeRepr<Point>, TypeRepr<Morphism>> {
	use scale_info::TypeDef::*;

	let ty1 = metadata1.types().resolve(ty1).unwrap().clone();
	let ty2 = metadata2.types().resolve(ty2).unwrap().clone();

	let diff_fields = |fields1: &Vec<Field<PortableForm>>, fields2: &Vec<Field<PortableForm>>| {
		let fields1: BTreeMap<_, _> = fields1
			.iter()
			.enumerate()
			.map(|(pos, field)| (field.name.clone(), (pos, field.ty.id)))
			.collect();
		let fields2: BTreeMap<_, _> = fields2
			.iter()
			.enumerate()
			.map(|(pos, field)| (field.name.clone(), (pos, field.ty.id)))
			.collect();

		let diff = diff(fields1, fields2);
		diff.into_iter()
			.map(|(name, d)| {
				let name = name.unwrap_or("".to_string());
				match d {
					NodeDiff::Left((pos, ty)) => CompactDiff::Removed(StructField {
						name,
						ty: TypeRepr::NotImplemented,
						position: pos,
					}),
					NodeDiff::Right((pos, ty)) => CompactDiff::Added(StructField {
						name,
						ty: TypeRepr::NotImplemented,
						position: pos,
					}),
					NodeDiff::Both((pos1, ty1), (pos2, ty2)) => {
						let type_diff = compare_types(metadata1, ty1, metadata2, ty2);

						CompactDiff::Inherited(StructField { name, position: pos1, ty: type_diff })
						// if a == b {
						// 	CompactDiff::Unchanged(StructField { name: name, ty:
						// TypeRepr::NotImplemented }) } else {
						// 	CompactDiff::Change(StructField { name: name.clone(), ty:
						// TypeRepr::NotImplemented }, StructField { name: name, ty:
						// TypeRepr::NotImplemented }) }
					},
				}
			})
			.collect::<Vec<_>>()
	};

	match (ty1.type_def, ty2.type_def) {
		(Composite(ty1), Composite(ty2)) => CompactDiff::compact_inherited(TypeRepr::Struct {
			fields: diff_fields(&ty1.fields, &ty2.fields),
		}),
		(Variant(ty1), Variant(ty2)) => {
			let variants1: BTreeMap<_, _> = ty1
				.variants
				.iter()
				.map(|variant| (variant.name.clone(), (variant.index, variant.fields.clone())))
				.collect();
			let variants2: BTreeMap<_, _> = ty2
				.variants
				.iter()
				.map(|variant| (variant.name.clone(), (variant.index, variant.fields.clone())))
				.collect();

			let diff = diff(variants1, variants2);
			CompactDiff::compact_inherited(TypeRepr::Enum {
				variants: diff
					.into_iter()
					.map(|(name, d)| match d {
						NodeDiff::Left((pos, fields)) =>
							CompactDiff::Removed(EnumVariant { name, fields: vec![] }),
						NodeDiff::Right((pos, fields)) =>
							CompactDiff::Unchanged(EnumVariant { name, fields: vec![] }),
						NodeDiff::Both((pos1, fields1), (pos2, fields2)) =>
							CompactDiff::compact_inherited(EnumVariant {
								name,
								fields: diff_fields(&fields1, &fields2),
							}),
					})
					.collect(),
			})
		},
		(Sequence(ty1), Sequence(ty2)) => CompactDiff::Unchanged(TypeRepr::NotImplemented),
		(Array(ty1), Array(ty2)) => CompactDiff::Unchanged(TypeRepr::NotImplemented),
		(Tuple(ty1), Tuple(ty2)) => CompactDiff::Unchanged(TypeRepr::NotImplemented),
		(Primitive(ty1), Primitive(ty2)) =>
			if ty1 == ty2 {
				CompactDiff::Unchanged(TypeRepr::Primitive(ty1))
			} else {
				CompactDiff::Change(TypeRepr::Primitive(ty1), TypeRepr::Primitive(ty2))
			},
		(Compact(ty1), Compact(ty2)) => CompactDiff::Unchanged(TypeRepr::NotImplemented),
		(BitSequence(ty1), BitSequence(ty2)) => CompactDiff::Unchanged(TypeRepr::NotImplemented),
		(_, _) => CompactDiff::Change(TypeRepr::NotImplemented, TypeRepr::NotImplemented),
	}
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
	// diff.retain(|_key, value| match value {
	// 	NodeDiff::Both(v, w) if v == w => false,
	// 	_ => true,
	// });

	for (location, entry) in diff {
		print!("{}::{}: ", location.pallet, location.storage_name);
		match entry {
			NodeDiff::Left(_) => println!("DELETED"),
			NodeDiff::Right(paths) => println!("CREATED {paths:?}"),
			NodeDiff::Both(old_paths, new_paths) => {
				use PortableStorageEntryType::*;
				match (old_paths, new_paths) {
					// (Plain(hash_set), Plain(hash_set)) => println!(),
					// (Plain(hash_set), Map(hash_set, hash_set1)) => todo!(),
					// (Map(hash_set, hash_set1), Plain(hash_set)) => todo!(),
					(Map(hash_set, old_ty), Map(hash_set2, new_ty)) => {

						let diff = compare_types(&old_metadata, old_ty, &new_metadata, new_ty);

						let updated = diff.get_updated();

						if updated != Update::None {
							println!("MODIFIED Types: {updated:#?}");
						}

					},

					(Plain(old_ty), Plain(new_ty)) => {
						let diff = compare_types(&old_metadata, old_ty, &new_metadata, new_ty);
						let updated = diff.get_updated();

						if updated != Update::None {
							println!("MODIFIED Types: {updated:#?}");
						}

					},

					_ => println!("MODIFIED: other"),
				}
			},
		}
	}
}
