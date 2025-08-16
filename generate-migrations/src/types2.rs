use scale_info::{Type, TypeDefPrimitive};

trait Shape {
	type Next;
}

trait Shaper {
	type Next: Shaper;
	type Strl<S: Shape>;
	type Item<S: Shape>;
	type Disc<A>;
}

trait ShaperHom<U: Shaper, V: Shaper> {
	fn apply_strl<Sh: Shaped>(&self, x: U::Strl<Sh::Result<U>>) -> V::Strl<Sh::Result<V>>;
	fn apply_item<Sh: Shaped>(&self, x: U::Item<Sh::Result<U>>) -> Option<V::Item<Sh::Result<V>>>;
	fn apply_disc<A>(&self, x: U::Disc<A>) -> V::Disc<A>;
}

trait Shaped {
	type Result<X: Shaper>: Shape<Next = Self::Result<X::Next>>;
	fn map<X: Shaper, Y: Shaper>(f: impl ShaperHom<X, Y>, x: Self::Result<X>) -> Self::Result<Y>;
}

//--------------------------------------------
// various diff methods

pub enum StructuralDiff<S: Shape> {
	Unchanged(S::Next),
	Change(S::Next, S::Next),
	Inherited(S),
}

#[derive(Clone, PartialEq, Debug)]
pub enum ItemDiff<S: Shape> {
	Removed(S::Next),
	Added(S::Next),
	Change(S::Next, S::Next),
	Inherited(S),
	Unchanged(S::Next),
}

pub enum DiscDiff<A> {
	Same(A),
	Changed(A, A),
}

type TypeName = String;
//--------------------------------------------
// definition of types

// -- struct field

pub struct StructField<S: Shaper> {
	pub pos: usize,
	pub name: S::Disc<Option<TypeName>>,
	pub ty: S::Strl<TypeExpr<S>>,
}

impl<S: Shaper> Shape for StructField<S> {
	type Next = StructField<S::Next>;
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

pub struct EnumVariant<S: Shaper> {
	pub pos: usize,
	pub name: S::Disc<TypeName>,
	pub content: S::Strl<StructField<S>>,
}

impl<S: Shaper> Shape for EnumVariant<S> {
	type Next = EnumVariant<S::Next>;
}

struct ShapedEnumVariant;
impl Shaped for ShapedEnumVariant {
	type Result<X: Shaper> = EnumVariant<X>;

	fn map<X: Shaper, Y: Shaper>(f: impl ShaperHom<X, Y>, x: Self::Result<X>) -> Self::Result<Y> {
		EnumVariant {
			pos: x.pos,
			name: f.apply_disc(x.name),
			content: f.apply_strl::<ShapedStructField>(x.content),
		}
	}
}

// -- typeexpr
pub enum TypeExpr<S: Shaper> {
	Struct { fields: Vec<S::Item<StructField<S>>> },
	Enum { variants: Vec<S::Item<EnumVariant<S>>> },
	Sequence { inner: Box<S::Strl<TypeExpr<S>>> },
	MapLike { key: Box<S::Strl<TypeExpr<S>>>, val: Box<S::Strl<TypeExpr<S>>> },
	Tuple { entries: Vec<S::Item<TypeExpr<S>>> },
	Primitive { prim: TypeDefPrimitive },
	NotImplemented,
}

impl<S: Shaper> Shape for TypeExpr<S> {
	type Next = TypeExpr<S::Next>;
}

struct ShapedTypeExpr;
impl Shaped for ShapedTypeExpr {
	type Result<X: Shaper> = TypeExpr<X>;

	fn map<X: Shaper, Y: Shaper>(f: impl ShaperHom<X, Y>, x: Self::Result<X>) -> Self::Result<Y> {
		match x {
			TypeExpr::Sequence { inner } =>
				TypeExpr::Sequence { inner: Box::new(f.apply_strl::<Self>(*inner)) },
			TypeExpr::Struct { fields } => TypeExpr::Struct {
				fields: fields
					.into_iter()
					.filter_map(|field| f.apply_item::<ShapedStructField>(field))
					.collect(),
			},
			TypeExpr::Enum { variants } => todo!(),
			TypeExpr::MapLike { key, val } => todo!(),
			TypeExpr::Tuple { entries } => todo!(),
			TypeExpr::Primitive { prim } => todo!(),
			TypeExpr::NotImplemented => todo!(),
		}
	}
}

// -- storage

//--------------------------------------------
// definition of shapers

pub struct Point;
impl Shaper for Point {
	type Next = Point;
	type Strl<S: Shape> = S;
	type Item<S: Shape> = S;
	type Disc<A> = A;
}

pub struct Morphism;
impl Shaper for Morphism {
	type Next = Point;
	type Strl<S: Shape> = StructuralDiff<S>;
	type Item<S: Shape> = ItemDiff<S>;
	type Disc<A> = DiscDiff<A>;
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

	fn apply_disc<A>(&self, x: <Morphism as Shaper>::Disc<A>) -> <Point as Shaper>::Disc<A> {
		match x {
			DiscDiff::Same(a) => a,
			DiscDiff::Changed(a, _) => a,
		}
	}
}

// ----- normalizing -----

pub fn normalize<X: Shaper>(x: TypeExpr<X>) -> TypeExpr<X> {
	todo!()
	// match x {
	//     TypeExpr::Struct { fields } => todo!(),
	//     TypeExpr::Sequence { inner } => todo!(),
	// }
}

// pub fn get_tuple2<X: Shaper>(x: TypeExpr<X>) -> Option<(TypeExpr<X>, TypeExpr<X>)> {
// }

// ---- diffing types ----

pub fn diff_typeexpr(x: TypeExpr<Point>, y: TypeExpr<Point>) -> TypeExpr<Morphism> {
	// match (x, y) {
	// }
	todo!()
}
