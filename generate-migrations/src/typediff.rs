use crate::diff::{NodeDiff, diff};
use codec::{Decode, Encode};
use derive_where::derive_where;
use frame_metadata::{RuntimeMetadata, v14::RuntimeMetadataV14};
use scale_info::{Field, MetaType, TypeDefPrimitive, form::PortableForm};
use state_chain_runtime::monitoring_apis::MonitoringDataV2;
use std::{
	any::Any,
	collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
	env::{self, var},
	fmt::Debug,
	fs,
	path::{Path, PathBuf, absolute},
	process,
	str::FromStr,
};
use subxt::{ext::scale_decode::visitor::types::Tuple, metadata::types::StorageEntryType};
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
	pallet: PalletInstanceRef,
	storage_name: String,
}

type FlatType = Vec<TypePath>;

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
enum PortableStorageEntryType<Ty> {
	Plain(Ty),
	Map(Ty, Ty),
}

pub fn get_all_storage_entries(
	config: &MetadataConfig,
	metadata: &subxt::Metadata,
) -> BTreeMap<StorageLocation, PortableStorageEntryType<u32>> {
	metadata
		.pallets()
		.filter(|pallet| config.include.iter().any(|string| pallet.name().contains(string)))
		.flat_map(|pallet| {
			pallet.storage().unwrap().entries().iter().cloned().map(move |entry| {
				(
					StorageLocation {
						pallet: PalletInstanceRef::from_string(pallet.name().to_string()),
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
pub enum CompactDiff<Point, Morphism> {
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

impl GetIdentity for TupleEntry<Morphism> {
	type Point = TupleEntry<Point>;

	fn try_get_identity(&self) -> Option<TupleEntry<Point>> {
		let TupleEntry { position, ty } = self;
		Some(TupleEntry { position: position.clone(), ty: ty.try_get_identity()? })
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
			variant: self.variant.try_get_identity()?,
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
pub trait CommonBounds = Debug + Clone + PartialEq;

pub trait CellType {
	type Of<Point: CommonBounds, Morphism: CommonBounds>: CommonBounds;
	type Discrete<Point: CommonBounds>: CommonBounds;
}

#[derive(Debug)]
pub struct Point;
impl CellType for Point {
	type Of<Point: CommonBounds, Morphism: CommonBounds> = Point;
	type Discrete<Point: CommonBounds> = Point;
}

#[derive(Debug)]
pub struct Morphism;
impl CellType for Morphism {
	type Of<Point: CommonBounds, Morphism: CommonBounds> = CompactDiff<Point, Morphism>;
	type Discrete<Point: CommonBounds> = DiscreteMorphism<Point>;
}

#[derive_where(Clone, PartialEq, Debug;)]
pub struct StructField<X: CellType> {
	pub name: String,
	pub position: usize,
	pub ty: X::Of<TypeRepr<Point>, TypeRepr<Morphism>>,
}

#[derive_where(Clone, PartialEq, Debug;)]
pub struct TupleEntry<X: CellType> {
	pub position: usize,
	pub ty: X::Of<TypeRepr<Point>, TypeRepr<Morphism>>,
}

#[derive_where(Clone, PartialEq, Debug;)]
pub struct EnumVariant<X: CellType> {
	pub variant: X::Discrete<String>,
	pub fields: Vec<X::Of<StructField<Point>, StructField<Morphism>>>,
}

#[derive(Debug, PartialEq, Clone, PartialOrd, Ord, Eq)]
pub enum TypeName {
	Ordinary { path: Vec<String>, name: String, chain: ChainInstance, params: Vec<TypeName> },
	VariantType { enum_type: Box<TypeName>, variant: String },
	Parameter { variable_name: String, value: Option<Box<TypeName>> },
	Unknown,
}

type ChainInstance = Option<String>;

impl TypeName {
	fn split_chain_instance(self) -> (Self, ChainInstance) {
		match self {
			TypeName::Ordinary { path, name, chain, params } =>
				(TypeName::Ordinary { path, name, chain: None, params }, chain),
			a @ TypeName::VariantType { .. } => (a, None),
			a @ TypeName::Unknown => (a, None),
			a @ TypeName::Parameter { .. } => (a, None),
		}
	}

	pub fn split_module(self) -> Option<(String, TypeName)> {
		match self {
			TypeName::Ordinary { path, name, chain, params } => {
				if let Some((module, rest)) = path.split_first() {
					Some((module.to_string(), TypeName::Ordinary { path: rest.to_vec(), name, chain, params }))
				} else {
					None
				}
			},
			a => None,
		}
	}
}

#[derive(Debug, PartialEq, Clone, PartialOrd, Ord, Eq)]
#[n_functor::derive_n_functor]
pub enum DiscreteMorphism<A> {
	Same(A),
	Rename { old: A, new: A },
}

type MaybeRenaming = DiscreteMorphism<TypeName>;

impl<A: PartialEq> DiscreteMorphism<A> {
	pub fn from_points(old: A, new: A) -> Self {
		if old == new { DiscreteMorphism::Same(old) } else { DiscreteMorphism::Rename { old, new } }
	}

	pub fn get_old(self) -> A {
		match self {
			DiscreteMorphism::Same(a) => a,
			DiscreteMorphism::Rename { old, new } => old,
		}
	}
}
impl<A, B> DiscreteMorphism<(A, B)> {
	pub fn split_tuple(self) -> (DiscreteMorphism<A>, DiscreteMorphism<B>) {
		use DiscreteMorphism::*;
		match self {
			Same((a, b)) => (Same(a), Same(b)),
			Rename { old: (olda, oldb), new: (newa, newb) } =>
				(Rename { old: olda, new: newa }, Rename { old: oldb, new: newb }),
		}
	}
}

#[derive_where(Clone, Debug, PartialEq;)]
pub enum TypeRepr<X: CellType> {
	Struct {
		typename: X::Discrete<TypeName>,
		fields: Vec<X::Of<StructField<Point>, StructField<Morphism>>>,
	},
	Enum {
		typename: X::Discrete<TypeName>,
		variants: Vec<X::Of<EnumVariant<Point>, EnumVariant<Morphism>>>,
	},
	Tuple {
		typename: X::Discrete<TypeName>,
		fields: Vec<X::Of<TupleEntry<Point>, TupleEntry<Morphism>>>,
	},
	Sequence {
		typename: X::Discrete<TypeName>,
		inner: Box<X::Of<TypeRepr<Point>, TypeRepr<Morphism>>>,
	},
	NotImplemented,
	Primitive(X::Of<TypeDefPrimitive, !>),
	TypeByName(X::Discrete<TypeName>),
}

impl<X: CellType> TypeRepr<X> {
	pub fn map_definition_typename(self, f: impl Fn(X::Discrete<TypeName>) -> X::Discrete<TypeName>) -> Self {
		match self {
			TypeRepr::Struct { typename, fields } => TypeRepr::Struct { typename: f(typename), fields },
			TypeRepr::Enum { typename, variants } => TypeRepr::Enum { typename: f(typename), variants },
			TypeRepr::Tuple { typename, fields } => TypeRepr::Tuple { typename, fields },
			TypeRepr::Sequence { typename, inner } => TypeRepr::Sequence { typename, inner },
			TypeRepr::NotImplemented => TypeRepr::NotImplemented,
			TypeRepr::Primitive(a) => TypeRepr::Primitive(a),
			TypeRepr::TypeByName(a) => TypeRepr::TypeByName(a),
		}
	}
}

#[derive(Debug, PartialEq, Clone)]
enum NodeKind {
	Struct,
	Enum,
	Sequence,
	Tuple,
}

#[derive(Debug, PartialEq, Clone)]
struct Migration {
	typename: MaybeRenaming,
	node_kind: NodeKind,
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
			node_kind: self.node_kind,
		}
	}
}

#[derive(Debug, PartialEq, Clone, PartialOrd, Ord, Eq)]
struct AbstractMigration {
	typename: MaybeRenaming,
	edits: Vec<String>,
	inner_migrations: BTreeMap<String, MaybeRenaming>,
}

#[derive(Debug, PartialEq, Clone, PartialOrd, Ord, Eq)]
struct AbstractMigrationInstance {
	storage: StorageLocation,
	type_path: TypePath2,
	type_chain_instance: DiscreteMorphism<Option<String>>,
}

impl AbstractMigrationInstance {
	fn prepend_path(self, prefix: String) -> Self {
		AbstractMigrationInstance {
			storage: self.storage,
			type_path: format!("{prefix}::{}", self.type_path),
			type_chain_instance: self.type_chain_instance,
		}
	}
}

type TypePath2 = String;

impl Migration {
	pub fn from_updates(
		node_kind: NodeKind,
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
			Some(Migration { node_kind, typename: typename.clone(), edits, inner_migrations })
		} else {
			None
		}
	}

	pub fn get_abstract_migrations(
		self,
		storage: StorageLocation,
	) -> BTreeMap<AbstractMigration, Vec<AbstractMigrationInstance>> {
		let mut result = BTreeMap::<AbstractMigration, Vec<AbstractMigrationInstance>>::new();
		for (abstract_migration, mut paths) in
			self.inner_migrations.into_iter().flat_map(|(field, migration)| {
				migration.get_abstract_migrations(storage.clone()).into_iter().map(
					move |(migration, instances)| {
						(
							migration,
							instances
								.into_iter()
								.map(|instance| instance.prepend_path(field.clone()))
								.collect(),
						)
					},
				)
			}) {
			result.entry(abstract_migration).or_default().append(&mut paths);
		}

		let (typename, chaininstance) =
			self.typename.map(|name| name.split_chain_instance()).split_tuple();

		if self.edits.len() > 0 {
			result.insert(
				AbstractMigration {
					typename,
					edits: self.edits,
					inner_migrations: Default::default(),
				},
				vec![AbstractMigrationInstance {
					storage,
					type_path: "".to_string(),
					type_chain_instance: chaininstance,
				}],
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
		node_kind: NodeKind,
		typename: &MaybeRenaming,
		updates: Vec<Option<PathUpdate>>,
	) -> Option<Update> {
		Migration::from_updates(node_kind, typename, updates).map(Update::Inner)
	}

	pub fn get_abstract_migrations(
		self,
		location: StorageLocation,
	) -> BTreeMap<AbstractMigration, Vec<AbstractMigrationInstance>> {
		match self {
			Update::Item(_) => Default::default(),
			Update::Inner(migration) => migration.get_abstract_migrations(location),
		}
	}

	pub fn with_prefix(self, prefix: String) -> Update {
		match self {
			Update::Item(x) => Update::Item(x),
			Update::Inner(migration) => Update::Inner(migration.prepend_path(prefix)),
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
		self.ty.get_update().map(|(path, update)| (format!("{}", self.name), update))
	}
}

impl GetUpdate for TupleEntry<Morphism> {
	type Item = PathUpdate;
	fn get_update(&self) -> Option<PathUpdate> {
		self.ty
			.get_update()
			.map(|(path, update)| (format!("{}", self.position), update))
	}
}

impl GetUpdate for EnumVariant<Morphism> {
	type Item = PathUpdate;
	fn get_update(&self) -> Option<PathUpdate> {
		// TODO, instead of doing this, we don't want to create migrations for single variants.
		Update::from_updates(
			NodeKind::Struct,
			&self.variant.clone().map(|name| TypeName::Ordinary {
				path: Default::default(),
				name: name,
				chain: None,
				params: vec![],
			}),
			self.fields
				.iter()
				.map(|f| f.get_update().map(|(path, update)| (path, update)))
				.collect(),
		)
		.map(|update| (format!("{:?}", self.variant), update))
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
				NodeKind::Struct,
				&typename,
				fields.iter().map(|field| field.get_update()).collect(),
			),
			TypeRepr::Enum { typename, variants } => Migration::from_updates(
				NodeKind::Enum,
				&typename,
				variants.iter().map(|field| field.get_update()).collect(),
			),
			TypeRepr::NotImplemented => None,
			TypeRepr::Primitive(x) => None,
			TypeRepr::TypeByName(_) => None,
			TypeRepr::Tuple { typename, fields } => Migration::from_updates(
				NodeKind::Tuple,
				&typename,
				fields.iter().map(|field| field.get_update()).collect(),
			),
			TypeRepr::Sequence { typename, inner } =>
				Migration::from_updates(NodeKind::Sequence, &typename, vec![inner.get_update()]),
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

fn type_to_string(metadata: &subxt::Metadata, ty: u32) -> TypeName {
	let ty = metadata.types().resolve(ty).unwrap();
	let path = ty.path.namespace();
	let mut name = ty.path.ident().unwrap_or_default();

	let mut chain_instance = None;
	for chain in CHAINS {
		if let Some(stripped) = name.strip_suffix(chain) {
			name = stripped.to_string();
			chain_instance = Some(chain.to_string());
			break;
		}
	}

	TypeName::Ordinary {
		path: path.to_vec(),
		name,
		chain: chain_instance,
		params: ty
			.type_params
			.clone()
			.into_iter()
			.map(|param| TypeName::Parameter {
				variable_name: param.name,
				value: param.ty.map(|ty| Box::new(type_to_string(metadata, ty.id))),
			})
			.collect(),
	}
}

impl<A: Clone> GetIdentity for DiscreteMorphism<A> {
	type Point = A;

	fn try_get_identity(&self) -> Option<Self::Point> {
		match self {
			DiscreteMorphism::Same(a) => Some(a.clone()),
			DiscreteMorphism::Rename { old, new } => None,
		}
	}
}

impl GetIdentity for TypeRepr<Morphism> {
	type Point = TypeRepr<Point>;

	fn try_get_identity(&self) -> Option<Self::Point> {
		match self {
			TypeRepr::Struct { typename, fields } =>
				if fields
					.iter()
					.map(|field| field.try_get_identity())
					.collect::<Option<Vec<_>>>()
					.is_some()
				{
					typename.try_get_identity().map(TypeRepr::TypeByName)
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
					typename.try_get_identity().map(TypeRepr::TypeByName)
				} else {
					None
				},
			TypeRepr::NotImplemented => None,
			TypeRepr::Primitive(type_def_primitive) => None,
			TypeRepr::TypeByName(_) => None,
			TypeRepr::Tuple { typename, fields } =>
				if fields
					.iter()
					.map(|field| field.try_get_identity())
					.collect::<Option<Vec<_>>>()
					.is_some()
				{
					typename.try_get_identity().map(TypeRepr::TypeByName)
				} else {
					None
				},
			TypeRepr::Sequence { typename, inner } =>
				if inner.try_get_identity().is_some() {
					typename.try_get_identity().map(TypeRepr::TypeByName)
				} else {
					None
				},
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

	let toplevel_typename = DiscreteMorphism::from_points(
		type_to_string(metadata1, ty1),
		type_to_string(metadata2, ty2),
	);

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
						ty: TypeRepr::TypeByName(type_to_string(metadata1, ty)),
						position: pos,
					}),
					NodeDiff::Right((pos, ty)) => CompactDiff::Added(StructField {
						name,
						ty: TypeRepr::TypeByName(type_to_string(metadata2, ty)),
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
				typename: toplevel_typename,
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
				variants: diff
					.into_iter()
					.map(|(name, d)| match d {
						NodeDiff::Left((pos, fields)) => CompactDiff::Removed(EnumVariant {
							variant: name.clone(),
							// typename: toplevel_typename.clone().map(|n| TypeName::VariantType {
							// 	enum_type: Box::new(n),
							// 	variant: name.clone(),
							// }),
							fields: vec![],
						}),
						NodeDiff::Right((pos, fields)) => CompactDiff::Added(EnumVariant {
							variant: name.clone(),
							// typename: toplevel_typename.clone().map(|n| TypeName::VariantType {
							// 	enum_type: Box::new(n),
							// 	variant: name.clone(),
							// }),
							fields: vec![],
						}),
						NodeDiff::Both((pos1, fields1), (pos2, fields2)) =>
						// TODO check that variant positions are the same!!!!
							CompactDiff::compact_inherited(EnumVariant {
								variant: DiscreteMorphism::Same(name.clone()),
								// typename: toplevel_typename.clone().map(|n| {
								// 	TypeName::VariantType {
								// 		enum_type: Box::new(n),
								// 		variant: name.clone(),
								// 	}
								// }),
								fields: diff_fields(&fields1, &fields2),
							}),
					})
					.collect(),
				typename: toplevel_typename,
			})
		},
		(Sequence(ty1), Sequence(ty2)) => {
			let type_diff =
				compare_types(metadata1, ty1.type_param.id, metadata2, ty2.type_param.id);
			CompactDiff::compact_inherited(TypeRepr::Sequence {
				inner: Box::new(type_diff),
				typename: toplevel_typename,
			})
		},
		(Array(ty1), Array(ty2)) => CompactDiff::Unchanged(TypeRepr::NotImplemented),
		(Tuple(entries1), Tuple(entries2)) => {
			let fields1: BTreeMap<_, _> =
				entries1.fields.iter().enumerate().map(|(pos, ty)| (pos, ty.id)).collect();
			let fields2: BTreeMap<_, _> =
				entries2.fields.iter().enumerate().map(|(pos, ty)| (pos, ty.id)).collect();

			let diff = diff(fields1, fields2);
			let fields = diff
				.into_iter()
				.map(|(pos, d)| match d {
					NodeDiff::Left(ty) => CompactDiff::Removed(TupleEntry {
						ty: TypeRepr::TypeByName(type_to_string(metadata1, ty)),
						position: pos,
					}),
					NodeDiff::Right(ty) => CompactDiff::Added(TupleEntry {
						ty: TypeRepr::TypeByName(type_to_string(metadata2, ty)),
						position: pos,
					}),
					NodeDiff::Both(ty1, ty2) => {
						let type_diff = compare_types(metadata1, ty1, metadata2, ty2);

						CompactDiff::Inherited(TupleEntry { position: pos, ty: type_diff })
					},
				})
				.collect::<Vec<_>>();

			// let typename = MaybeRenaming::from_strings(
			// 	entries1
			// 		.fields
			// 		.iter()
			// 		.map(|ty| type_to_string(metadata1, ty.id))
			// 		.intersperse(", ".to_string())
			// 		.collect::<Vec<_>>()
			// 		.concat(),
			// 	entries2
			// 		.fields
			// 		.iter()
			// 		.map(|ty| type_to_string(metadata2, ty.id))
			// 		.intersperse(", ".to_string())
			// 		.collect::<Vec<_>>()
			// 		.concat(),
			// );

			CompactDiff::compact_inherited(TypeRepr::Tuple { typename: toplevel_typename, fields })
		},
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

#[derive(clap::Args, Clone)]
pub struct MetadataConfig {
	#[arg(short, long, value_name = "INCLUDE")]
	include: Vec<String>,
}

pub fn extract_old_typename<A>(n: DiscreteMorphism<A>) -> A {
	match n {
		DiscreteMorphism::Same(a) => a,
		DiscreteMorphism::Rename { old, new } => old,
	}
}

pub fn extract_old_struct_field(f: StructField<Morphism>) -> StructField<Point> {
	StructField {
		name: f.name,
		position: f.position,
		ty: extract_old_diff(f.ty, extract_old_type).unwrap(),
	}
}

pub fn extract_old_enum_variant(v: EnumVariant<Morphism>) -> EnumVariant<Point> {
	EnumVariant {
		variant: extract_old_typename(v.variant),
		// typename: extract_old_typename(v.typename),
		fields: v
			.fields
			.into_iter()
			.filter_map(|x| extract_old_diff(x, extract_old_struct_field))
			.collect(),
	}
}

pub fn extract_old_tuple_entry(f: TupleEntry<Morphism>) -> TupleEntry<Point> {
	TupleEntry { position: f.position, ty: extract_old_diff(f.ty, extract_old_type).unwrap() }
}

pub fn extract_old_diff_maybe<A, B>(d: CompactDiff<A, B>, f: impl Fn(B) -> Option<A>) -> Option<A> {
	match d {
		CompactDiff::Removed(a) => Some(a),
		CompactDiff::Added(_) => None,
		CompactDiff::Change(a, _) => Some(a),
		CompactDiff::Inherited(x) => f(x),
		CompactDiff::Unchanged(a) => Some(a),
	}
}

pub fn extract_old_diff<A, B>(d: CompactDiff<A, B>, f: impl Fn(B) -> A) -> Option<A> {
	match d {
		CompactDiff::Removed(a) => Some(a),
		CompactDiff::Added(_) => None,
		CompactDiff::Change(a, _) => Some(a),
		CompactDiff::Inherited(x) => Some(f(x)),
		CompactDiff::Unchanged(a) => Some(a),
	}
}

pub fn extract_old_type(ty: TypeRepr<Morphism>) -> TypeRepr<Point> {
	match ty {
		TypeRepr::Struct { typename, fields } => TypeRepr::Struct {
			typename: extract_old_typename(typename),
			fields: fields
				.into_iter()
				.filter_map(|d| extract_old_diff(d, extract_old_struct_field))
				.collect(),
		},
		TypeRepr::Enum { typename, variants } => TypeRepr::Enum {
			typename: extract_old_typename(typename),
			variants: variants
				.into_iter()
				.filter_map(|d| extract_old_diff(d, extract_old_enum_variant))
				.collect(),
		},
		TypeRepr::Tuple { typename, fields } => TypeRepr::Tuple {
			typename: extract_old_typename(typename),
			fields: fields
				.into_iter()
				.filter_map(|d| extract_old_diff(d, extract_old_tuple_entry))
				.collect(),
		},
		TypeRepr::Sequence { typename, inner } => TypeRepr::Sequence {
			typename: extract_old_typename(typename),
			inner: Box::new(extract_old_diff(*(inner.clone()), extract_old_type).unwrap()),
		},
		TypeRepr::NotImplemented => TypeRepr::NotImplemented,
		TypeRepr::Primitive(a) =>
			TypeRepr::Primitive(extract_old_diff(a, |_| TypeDefPrimitive::Bool).unwrap()),
		TypeRepr::TypeByName(n) => TypeRepr::TypeByName(extract_old_typename(n)),
	}
}

pub fn extract_old_if_changed(
	diff: &CompactDiff<TypeRepr<Point>, TypeRepr<Morphism>>,
) -> Option<TypeRepr<Point>> {
	match diff {
		CompactDiff::Removed(a) => Some(a.clone()),
		CompactDiff::Added(_) => None,
		CompactDiff::Change(a, _) => Some(a.clone()),
		CompactDiff::Inherited(f) => Some(extract_old_type(f.clone())),
		CompactDiff::Unchanged(_) => None,
	}
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub struct PalletInstanceRef {
	pub name: String,
	pub chain_instance: Option<String>,
}

impl PalletInstanceRef {
	pub fn from_string(mut name: String) -> Self {
		let mut chain_instance = None;
		for chain in CHAINS {
			if let Some(stripped) = name.strip_prefix(chain) {
				name = stripped.to_string();
				chain_instance = Some(chain.to_string());
				break;
			}
		}

		PalletInstanceRef { name, chain_instance }
	}
}

pub type PalletRef = String;
pub type PalletInstanceContent = Vec<TypeRepr<Point>>;

pub struct MetadataResult {
	pub old_definitions: BTreeMap<PalletInstanceRef, PalletInstanceContent>,
}

pub async fn compare_metadata(config: &MetadataConfig) -> MetadataResult {
	let metadata = state_chain_runtime::Runtime::metadata().1;
	let pallets: Vec<_> = match metadata {
		RuntimeMetadata::V14(runtime_metadata_v14) =>
			runtime_metadata_v14.pallets.into_iter().map(|pallet| pallet.name).collect(),
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

	// println!("online pallets: {:?}", get_pallet_names(old_metadata.clone()));
	// println!("local  pallets: {:?}", get_pallet_names(new_metadata.clone()));

	// compute storage objects that differ
	let old_storage = get_all_storage_entries(config, &old_metadata);
	let new_storage = get_all_storage_entries(config, &new_metadata);
	let diff = crate::diff::diff(old_storage, new_storage);

	let mut abstract_migrations =
		BTreeMap::<AbstractMigration, Vec<AbstractMigrationInstance>>::new();

	let mut old_definitions = BTreeMap::<PalletInstanceRef, Vec<TypeRepr<Point>>>::new();

	for (location, entry) in diff {
		// print!("{}::{}: ", location.pallet, location.storage_name);
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

						// --- old defs ---
						if let Some(old) = extract_old_if_changed(&diff) {
							old_definitions.entry(location.pallet.clone()).or_default().push(old);
						}

						// --- migrations ---
						let updated = diff
							.get_update()
							.map(|(path, m)| Update::get_abstract_migrations(m, location))
							.unwrap_or_default();

						if updated.len() > 0 {
							for (m, mut paths) in updated {
								abstract_migrations.entry(m).or_default().extend(paths.into_iter());
							}
						}
					},

					(Plain(old_ty), Plain(new_ty)) => {
						let diff = compare_types(&old_metadata, old_ty, &new_metadata, new_ty);

						// --- old defs ---
						if let Some(old) = extract_old_if_changed(&diff) {
							old_definitions.entry(location.pallet.clone()).or_default().push(old);
						}

						// --- migrations ---
						let updated = diff
							.get_update()
							.map(|(path, m)| Update::get_abstract_migrations(m, location))
							.unwrap_or_default();

						if updated.len() > 0 {
							for (m, paths) in updated {
								abstract_migrations.entry(m).or_default().extend(paths.into_iter());
							}
						}
					},

					_ => println!("MODIFIED: other"),
				}
			},
		}
	}

	// println!("Migrations: \n {abstract_migrations:#?}");

	MetadataResult { old_definitions }
}

const CHAINS: [&'static str; 6] =
	["Bitcoin", "Ethereum", "Arbitrum", "Solana", "Polkadot", "Assethub"];
