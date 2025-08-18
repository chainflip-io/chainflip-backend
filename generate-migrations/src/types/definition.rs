use derive_where::derive_where;
use scale_info::TypeDefPrimitive;
use std::{collections::BTreeMap, fmt::Debug};

pub trait CommonTraits = Clone + Debug + Eq + Ord;

pub trait Shape: CommonTraits {
	type Next: CommonTraits;
	fn try_get_next(&self) -> Option<Self::Next>;
}

pub trait Shaper {
	type Next: Shaper;
	type Strl<S: Shape>: CommonTraits;
	type Item<S: Shape>: CommonTraits;
	type Disc<A: CommonTraits>: CommonTraits;
}

trait ShaperHom<U: Shaper, V: Shaper> {
	fn apply_strl<Sh: Shaped>(&self, x: U::Strl<Sh::Result<U>>) -> V::Strl<Sh::Result<V>>;
	fn apply_item<Sh: Shaped>(&self, x: U::Item<Sh::Result<U>>) -> Option<V::Item<Sh::Result<V>>>;
	fn apply_disc<A: CommonTraits>(&self, x: U::Disc<A>) -> V::Disc<A>;
}

trait Shaped {
	type Result<X: Shaper>: Shape<Next = Self::Result<X::Next>>;
	fn map<X: Shaper, Y: Shaper>(f: impl ShaperHom<X, Y>, x: Self::Result<X>) -> Self::Result<Y>;
}

//--------------------------------------------
// various diff methods

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum StructuralDiff<S: Shape> {
	Unchanged(S::Next),
	Change(S::Next, S::Next),
	Inherited(S),
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum ItemDiff<S: Shape> {
	Removed(S::Next),
	Added(S::Next),
	Change(S::Next, S::Next),
	Inherited(S),
	Unchanged(S::Next),
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum DiscDiff<A> {
	Same(A),
	Changed(A, A),
}

type TypeName = String;
//--------------------------------------------
// definition of types

// -- struct field

#[derive_where(Debug, Clone, PartialEq, Eq, PartialOrd, Ord; )]
pub struct StructField<S: Shaper> {
	pub pos: usize,
	pub name: S::Disc<Option<TypeName>>,
	pub ty: S::Strl<TypeExpr<S>>,
}

impl<S: Shaper> Shape for StructField<S> {
	type Next = StructField<S::Next>;
	
	fn try_get_next(&self) -> Option<Self::Next> {
		todo!()
	}
}

struct ShapedStructField;
impl Shaped for ShapedStructField {
	type Result<X: Shaper> = StructField<X>;

	fn map<X: Shaper, Y: Shaper>(f: impl ShaperHom<X, Y>, x: Self::Result<X>) -> Self::Result<Y> {
		let StructField { pos, name, ty } = x;
		StructField { pos, name: f.apply_disc(name), ty: f.apply_strl::<ShapedTypeExpr>(ty) }
	}
}

// -- enum variant

#[derive_where(Clone, PartialEq, Eq, PartialOrd, Ord, Debug;)]
pub struct EnumVariant<S: Shaper> {
	pub pos: usize,
	pub name: S::Disc<TypeName>,
	pub fields: Vec<S::Item<StructField<S>>>,
}

impl<S: Shaper> Shape for EnumVariant<S> {
	type Next = EnumVariant<S::Next>;
	
	fn try_get_next(&self) -> Option<Self::Next> {
			todo!()
		}
}

struct ShapedEnumVariant;
impl Shaped for ShapedEnumVariant {
	type Result<X: Shaper> = EnumVariant<X>;

	fn map<X: Shaper, Y: Shaper>(f: impl ShaperHom<X, Y>, x: Self::Result<X>) -> Self::Result<Y> {
		EnumVariant {
			pos: x.pos,
			name: f.apply_disc(x.name),
			fields: x
				.fields
				.into_iter()
				.filter_map(|field| f.apply_item::<ShapedStructField>(field))
				.collect(),
		}
	}
}

// -- typeexpr
#[derive_where(Debug, Clone, PartialEq, Eq, PartialOrd, Ord; )]
pub enum TypeExpr<S: Shaper> {
	Struct { fields: Vec<S::Item<StructField<S>>> },
	Enum { variants: Vec<S::Item<EnumVariant<S>>> },
	VecLike { inner: Box<S::Strl<TypeExpr<S>>> },
	MapLike { key: Box<S::Strl<TypeExpr<S>>>, val: Box<S::Strl<TypeExpr<S>>> },
	Tuple { entries: Vec<S::Item<TypeExpr<S>>> },
	Primitive { prim: TypeDefPrimitive },
	NotImplemented,
}

impl<S: Shaper> Shape for TypeExpr<S> {
	type Next = TypeExpr<S::Next>;
	
	fn try_get_next(&self) -> Option<Self::Next> {
		todo!()
	}
}

struct ShapedTypeExpr;
impl Shaped for ShapedTypeExpr {
	type Result<X: Shaper> = TypeExpr<X>;

	fn map<X: Shaper, Y: Shaper>(f: impl ShaperHom<X, Y>, x: Self::Result<X>) -> Self::Result<Y> {
		match x {
			TypeExpr::VecLike { inner } =>
				TypeExpr::VecLike { inner: Box::new(f.apply_strl::<ShapedTypeExpr>(*inner)) },
			TypeExpr::Struct { fields } => TypeExpr::Struct {
				fields: fields
					.into_iter()
					.filter_map(|field| f.apply_item::<ShapedStructField>(field))
					.collect(),
			},
			TypeExpr::Enum { variants } => TypeExpr::Enum {
				variants: variants
					.into_iter()
					.filter_map(|variant| f.apply_item::<ShapedEnumVariant>(variant))
					.collect(),
			},
			TypeExpr::MapLike { key, val } => TypeExpr::MapLike {
				key: Box::new(f.apply_strl::<ShapedTypeExpr>(*key)),
				val: Box::new(f.apply_strl::<ShapedTypeExpr>(*val)),
			},
			TypeExpr::Tuple { entries } => TypeExpr::Tuple {
				entries: entries
					.into_iter()
					.filter_map(|entry| f.apply_item::<ShapedTypeExpr>(entry))
					.collect(),
			},
			TypeExpr::Primitive { prim } => TypeExpr::Primitive { prim },
			TypeExpr::NotImplemented => TypeExpr::NotImplemented,
		}
	}
}

// -- storage entry
#[derive_where(Debug, Clone, PartialEq, Eq, PartialOrd, Ord; )]
pub enum StorageEntry<S: Shaper> {
	Value(Box<S::Strl<TypeExpr<S>>>),
	Map(Box<S::Strl<TypeExpr<S>>>, Box<S::Strl<TypeExpr<S>>>)
}

impl<S: Shaper> Shape for StorageEntry<S> {
	type Next = StorageEntry<S::Next>;

	fn try_get_next(&self) -> Option<Self::Next> {
		todo!()
	}
}

struct ShapedStorageEntry;
impl Shaped for ShapedStorageEntry {
	type Result<X: Shaper> = StorageEntry<X>;

	fn map<X: Shaper, Y: Shaper>(f: impl ShaperHom<X, Y>, x: Self::Result<X>) -> Self::Result<Y> {
		match x {
			StorageEntry::Value(v) => StorageEntry::Value(Box::new(f.apply_strl::<ShapedTypeExpr>(*v))),
			StorageEntry::Map(k, v) => StorageEntry::Map(
				Box::new(f.apply_strl::<ShapedTypeExpr>(*k)),
				Box::new(f.apply_strl::<ShapedTypeExpr>(*v))
			),
		}
	}
}

// -- pallet 

#[derive_where(Debug, Clone, PartialEq, Eq, PartialOrd, Ord; )]
pub struct PalletStorage<S: Shaper> {
	pub entries: BTreeMap<Name, S::Item<StorageEntry<S>>>
}

impl<S: Shaper> Shape for PalletStorage<S> {
	type Next = PalletStorage<S::Next>;

	fn try_get_next(&self) -> Option<Self::Next> {
		todo!()
	}
}

struct ShapedPalletStorage;
impl Shaped for ShapedPalletStorage {
	type Result<X: Shaper> = PalletStorage<X>;

	fn map<X: Shaper, Y: Shaper>(f: impl ShaperHom<X, Y>, x: Self::Result<X>) -> Self::Result<Y> {
		PalletStorage {
			entries: x.entries.into_iter().filter_map(|entry| f.apply_item::<ShapedStorageEntry>(entry)).collect(),
		}
	}
}


//--------------------------------------------
// shaper instances

pub struct Point;
impl Shaper for Point {
	type Next = Point;
	type Strl<S: Shape> = S;
	type Item<S: Shape> = S;
	type Disc<A: CommonTraits> = A;
}

pub struct Morphism;
impl Shaper for Morphism {
	type Next = Point;
	type Strl<S: Shape> = StructuralDiff<S>;
	type Item<S: Shape> = ItemDiff<S>;
	type Disc<A: CommonTraits> = DiscDiff<A>;
}

// ---- proj to old ----

#[derive(Clone)]
struct ProjOld;

impl ShaperHom<Morphism, Point> for ProjOld {
	fn apply_strl<Sh: Shaped>(
		&self,
		x: <Morphism as Shaper>::Strl<Sh::Result<Morphism>>,
	) -> <Point as Shaper>::Strl<Sh::Result<Point>> {
		match x {
			StructuralDiff::Unchanged(a) => a,
			StructuralDiff::Change(a, b) => a,
			StructuralDiff::Inherited(a) => Sh::map(self.clone(), a),
		}
	}

	fn apply_item<Sh: Shaped>(
		&self,
		x: <Morphism as Shaper>::Item<Sh::Result<Morphism>>,
	) -> Option<<Point as Shaper>::Item<Sh::Result<Point>>> {
		match x {
			ItemDiff::Removed(a) => Some(a),
			ItemDiff::Added(_) => None,
			ItemDiff::Change(a, _) => Some(a),
			ItemDiff::Inherited(a) => Some(Sh::map(self.clone(), a)),
			ItemDiff::Unchanged(a) => Some(a),
		}
	}

	fn apply_disc<A: CommonTraits>(
		&self,
		x: <Morphism as Shaper>::Disc<A>,
	) -> <Point as Shaper>::Disc<A> {
		match x {
			DiscDiff::Same(a) => a,
			DiscDiff::Changed(a, _) => a,
		}
	}
}

// pub fn get_tuple2<X: Shaper>(x: TypeExpr<X>) -> Option<(TypeExpr<X>, TypeExpr<X>)> {
// }

// ---- diffing types ----
