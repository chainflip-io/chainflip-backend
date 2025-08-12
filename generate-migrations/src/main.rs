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
			typename: self.typename.clone(),
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
	typename: MaybeRenaming,
	fields: Vec<X::Of<StructField<Point>, StructField<Morphism>>>,
}

type TypeName = String;

#[derive(Debug, PartialEq, Clone, PartialOrd, Ord, Eq)]
enum MaybeRenaming {
	Same(TypeName),
	Rename { old: TypeName, new: TypeName },
}

impl MaybeRenaming {
	pub fn from_strings(old: String, new: String) -> Self {
		if old == new { MaybeRenaming::Same(old) } else { MaybeRenaming::Rename { old, new } }
	}
	// pub fn old(&self) -> TypeName {
	// 	match self {
	// 		MaybeRenaming::Same(a) => a.clone(),
	// 		MaybeRenaming::Rename { old, new } => old.clone(),
	// 	}
	// }
	// pub fn new(&self) -> TypeName {
	// 	match self {
	// 		MaybeRenaming::Same(a) => a.clone(),
	// 		MaybeRenaming::Rename { old, new } => new.clone(),
	// 	}
	// }
}

#[derive_where(Clone, Debug, PartialEq;)]
enum TypeRepr<X: CellType> {
	Struct {
		typename: MaybeRenaming,
		fields: Vec<X::Of<StructField<Point>, StructField<Morphism>>>,
	},
	Enum {
		typename: MaybeRenaming,
		variants: Vec<X::Of<EnumVariant<Point>, EnumVariant<Morphism>>>,
	},
	NotImplemented,
	Primitive(X::Of<TypeDefPrimitive, !>),
	TypeByName(MaybeRenaming),
}

#[derive(Debug, PartialEq, Clone)]
struct Migration {
	typename: MaybeRenaming,
	edits: Vec<String>,
	inner_migrations: BTreeMap<String, Migration>,
}

impl Migration {
	pub fn prepend_path(self, prefix: String) -> Self {
		Migration {
			typename: self.typename,
			edits: self.edits,
			inner_migrations: self
				.inner_migrations
				.into_iter()
				.map(|(path, m)| (format!("{prefix}::{path}"), m))
				.collect(),
		}
	}
}

#[derive(Debug, PartialEq, Clone, PartialOrd, Ord, Eq)]
struct AbstractMigration {
	typename: MaybeRenaming,
	edits: Vec<String>,
	inner_migrations: BTreeMap<String, MaybeRenaming>,
}

type TypePath2 = String;

impl Migration {
	pub fn from_updates(
		typename: &MaybeRenaming,
		updates: Vec<Option<PathUpdate>>,
	) -> Option<Migration> {
		let edits = updates
			.iter()
			.filter_map(|update| match update {
				Some((path, Update::Item(a))) => Some(a.clone()),
				Some((path, Update::Inner(migrations))) => None,
				None => None,
			})
			.collect::<Vec<_>>();

		let inner_migrations = updates
			.iter()
			.filter_map(|update| match update {
				Some((path, Update::Item(_))) => None,
				Some((path, Update::Inner(migration))) => Some((path.clone(), migration.clone())),
				None => None,
			})
			.collect::<BTreeMap<_, _>>();

		if edits.len() > 0 || inner_migrations.len() > 0 {
			Some(Migration { typename: typename.clone(), edits, inner_migrations })
		} else {
			None
		}
	}

	pub fn get_abstract_migrations(self) -> BTreeMap<AbstractMigration, Vec<TypePath2>> {
		let mut result = BTreeMap::<AbstractMigration, Vec<TypePath2>>::new();
		for (abstract_migration, mut paths) in
			self.inner_migrations.into_iter().flat_map(|(field, migration)| {
				migration.get_abstract_migrations().into_iter().map(move |(migration, paths)| {
					(migration, paths.into_iter().map(|path| format!("{field}::{path}")).collect())
				})
			}) {
			result.entry(abstract_migration).or_default().append(&mut paths);
		}

		if self.edits.len() > 0 {
			result.insert(
				AbstractMigration {
					typename: self.typename,
					edits: self.edits,
					inner_migrations: Default::default(),
				},
				vec!["".to_string()],
			);
		}

		result
	}
}

#[derive(Debug, PartialEq, Clone)]
enum Update {
	Item(String),
	Inner(Migration),
}

impl Update {
	pub fn from_updates(
		typename: &MaybeRenaming,
		updates: Vec<Option<PathUpdate>>,
	) -> Option<Update> {
		Migration::from_updates(typename, updates).map(Update::Inner)
	}

	pub fn get_abstract_migrations(self) -> BTreeMap<AbstractMigration, Vec<TypePath2>> {
		match self {
			Update::Item(_) => Default::default(),
			Update::Inner(migration) => migration.get_abstract_migrations(),
		}
	}
}

impl<P: Debug, M: GetUpdate<Item = PathUpdate>> GetUpdate for CompactDiff<P, M> {
	type Item = PathUpdate;
	fn get_update(&self) -> Option<PathUpdate> {
		match self {
			CompactDiff::Removed(b) => Some(("".to_string(), Update::Item(format!("- {b:?}")))),
			CompactDiff::Added(a) => Some(("".to_string(), Update::Item(format!("+ {a:?}")))),
			CompactDiff::Change(a, b) =>
				Some(("".to_string(), Update::Item(format!("C {a:?} => {b:?}")))),
			CompactDiff::Inherited(f) => f.get_update(),
			CompactDiff::Unchanged(_) => None,
		}
	}
}

impl GetUpdate for StructField<Morphism> {
	type Item = PathUpdate;
	fn get_update(&self) -> Option<PathUpdate> {
		self.ty
			.get_update()
			.map(|(path, update)| (format!("{}::{path}", self.name), update))
	}
}

impl GetUpdate for EnumVariant<Morphism> {
	type Item = PathUpdate;
	fn get_update(&self) -> Option<PathUpdate> {
		Update::from_updates(
			&self.typename,
			self.fields
				.iter()
				.map(|f| f.get_update().map(|(path, update)| (path, update)))
				.collect(),
		)
		.map(|update| (format!("{:?}", self.typename), update))
	}
}

impl GetUpdate for TypeRepr<Morphism> {
	type Item = PathUpdate;

	fn get_update(&self) -> Option<Self::Item> {
		self.get_migration().map(|m| ("".to_string(), Update::Inner(m)))
	}
}

impl GetMigration for TypeRepr<Morphism> {
	fn get_migration(&self) -> Option<Migration> {
		match self {
			TypeRepr::Struct { typename, fields } => Migration::from_updates(
				&typename,
				fields.iter().map(|field| field.get_update()).collect(),
			),
			TypeRepr::Enum { typename, variants } => Migration::from_updates(
				&typename,
				variants.iter().map(|field| field.get_update()).collect(),
			),
			TypeRepr::NotImplemented => None,
			TypeRepr::Primitive(x) => None,
			TypeRepr::TypeByName(_) => None,
		}
	}
}

trait GetUpdate {
	type Item;
	fn get_update(&self) -> Option<Self::Item>;
}

type PathUpdate = (String, Update);

trait GetMigration {
	fn get_migration(&self) -> Option<Migration>;
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
			TypeRepr::Struct { typename, fields } =>
				if fields
					.iter()
					.map(|field| field.try_get_identity())
					.collect::<Option<Vec<_>>>()
					.is_some()
				{
					Some(TypeRepr::TypeByName(typename.clone()))
				} else {
					None
				},
			TypeRepr::Enum { typename, variants } =>
				if variants
					.iter()
					.map(|field| field.try_get_identity())
					.collect::<Option<Vec<_>>>()
					.is_some()
				{
					Some(TypeRepr::TypeByName(typename.clone()))
				} else {
					None
				},
			TypeRepr::NotImplemented => None,
			TypeRepr::Primitive(type_def_primitive) => None,
			TypeRepr::TypeByName(_) => None,
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
		(Composite(ty1content), Composite(ty2content)) =>
			CompactDiff::compact_inherited(TypeRepr::Struct {
				fields: diff_fields(&ty1content.fields, &ty2content.fields),
				typename: MaybeRenaming::from_strings(ty1.path.to_string(), ty2.path.to_string()),
			}),
		(Variant(ty1content), Variant(ty2content)) => {
			let variants1: BTreeMap<_, _> = ty1content
				.variants
				.iter()
				.map(|variant| (variant.name.clone(), (variant.index, variant.fields.clone())))
				.collect();
			let variants2: BTreeMap<_, _> = ty2content
				.variants
				.iter()
				.map(|variant| (variant.name.clone(), (variant.index, variant.fields.clone())))
				.collect();

			let diff = diff(variants1, variants2);
			CompactDiff::compact_inherited(TypeRepr::Enum {
				typename: MaybeRenaming::from_strings(ty1.path.to_string(), ty2.path.to_string()),
				variants: diff
					.into_iter()
					.map(|(name, d)| match d {
						NodeDiff::Left((pos, fields)) => CompactDiff::Removed(EnumVariant {
							typename: MaybeRenaming::Same(name.clone()),
							fields: vec![],
						}),
						NodeDiff::Right((pos, fields)) => CompactDiff::Unchanged(EnumVariant {
							typename: MaybeRenaming::Same(name.clone()),
							fields: vec![],
						}),
						NodeDiff::Both((pos1, fields1), (pos2, fields2)) =>
							CompactDiff::compact_inherited(EnumVariant {
								typename: MaybeRenaming::Same(name.clone()),
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

	let mut abstract_migrations = BTreeMap::<AbstractMigration, Vec<TypePath2>>::new();

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

						let updated = diff
							.get_update()
							.map(|(path, m)| Update::get_abstract_migrations(m))
							.unwrap_or_default();

						if updated.len() > 0 {
							for (m, mut paths) in updated {
								abstract_migrations.entry(m).or_default().extend(
									paths.into_iter().map(|path| {
										format!(
											"{}::{}::{path}",
											location.pallet, location.storage_name
										)
									}),
								);
							}
							// println!("MODIFIED Types: {updated:#?}");
						}
					},

					(Plain(old_ty), Plain(new_ty)) => {
						let diff = compare_types(&old_metadata, old_ty, &new_metadata, new_ty);
						let updated = diff
							.get_update()
							.map(|(path, m)| Update::get_abstract_migrations(m))
							.unwrap_or_default();

						if updated.len() > 0 {
							for (m, paths) in updated {
								abstract_migrations.entry(m).or_default().extend(
									paths.into_iter().map(|path| {
										format!(
											"{}::{}::{path}",
											location.pallet, location.storage_name
										)
									}),
								);
							}
							// println!("MODIFIED Types: {updated:#?}");
						}
					},

					_ => println!("MODIFIED: other"),
				}
			},
		}
	}

	println!("Migrations: \n {abstract_migrations:#?}");
}
