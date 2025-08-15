

trait Shape {
    type Next;
}

trait Shaper {
    type Next: Shaper;
    type Strl<S: Shape>;
    type Vec<S: Shape>;
    type Disc<A>;
}

trait ShaperHom<U: Shaper, V: Shaper> {
    fn apply_strl<Sh: Shaped>(&self, x: U::Strl<Sh::Result<U>>) -> V::Strl<Sh::Result<V>>;
    fn apply_vec<Sh: Shaped>(&self, x: U::Vec<Sh::Result<U>>) -> V::Vec<Sh::Result<V>>;
    fn apply_disc<A>(&self, x: U::Disc<A>) -> V::Disc<A>;
}

trait Shaped {
    type Result<X: Shaper>: Shape<Next = Self::Result<X::Next>>;
    fn map<X: Shaper, Y: Shaper>(f: impl ShaperHom<X,Y>, x: Self::Result<X>) -> Self::Result<Y>;
}

//--------------------------------------------
// various diff methods

pub enum StructuralDiff<S: Shape> {
    Unchanged(S::Next),
	Change(S::Next, S::Next),
    Inherited(S)
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
    Changed(A,A)
}


type TypeName = String;
//--------------------------------------------
// definition of types

pub struct StructField<S: Shaper> {
    pos: usize,
    name: S::Disc<TypeName>,
    ty: S::Strl<TypeExpr<S>>
}

impl<S: Shaper> Shape for StructField<S> {
    type Next = StructField<S::Next>;
}

pub enum TypeExpr<S: Shaper> {
    Struct {
        fields: S::Vec<StructField<S>>
    },
    Sequence {
        inner: S::Strl<TypeExpr<S>>
    }
}

impl<S: Shaper> Shape for TypeExpr<S> {
    type Next = TypeExpr<S::Next>;
}

//--------------------------------------------
// definition of shapers

struct Point;
impl Shaper for Point {
    type Next = Point;
    type Strl<S: Shape> = S;
    type Vec<S: Shape> = Vec<S>;
    type Disc<A> = A;
}

struct Morphism;
impl Shaper for Morphism {
    type Next = Point;
    type Strl<S: Shape> = StructuralDiff<S>;
    type Vec<S: Shape> = Vec<ItemDiff<S>>;
    type Disc<A> = DiscDiff<A>;
}



struct ShapedTypeExpr;
impl Shaped for ShapedTypeExpr {
    type Result<X: Shaper> = TypeExpr<X>;

    fn map<X: Shaper, Y: Shaper>(f: impl ShaperHom<X,Y>, x: Self::Result<X>) -> Self::Result<Y> {
        match x {
            TypeExpr::Sequence { inner } => TypeExpr::Sequence { inner: f.apply_strl::<Self>(inner) },
            TypeExpr::Struct { fields } => TypeExpr::Struct { fields: f.apply_vec::<ShapedStructField>(fields) },
        }
    }
}

struct ShapedStructField;
impl Shaped for ShapedStructField {
    type Result<X: Shaper> = StructField<X>;

    fn map<X: Shaper, Y: Shaper>(f: impl ShaperHom<X,Y>, x: Self::Result<X>) -> Self::Result<Y> {
        let StructField { pos, name, ty } = x;
        StructField {
            pos,
            name: f.apply_disc(name),
            ty: f.apply_strl::<ShapedTypeExpr>(ty),
        }
    }
}

// ---- proj to old ----

#[derive(Clone)]
struct ProjOld;

impl ShaperHom<Morphism,Point> for ProjOld {
    fn apply_strl<Sh: Shaped>(&self, x: <Morphism as Shaper>::Strl<Sh::Result<Morphism>>) -> <Point as Shaper>::Strl<Sh::Result<Point>> {
        match x {
            StructuralDiff::Unchanged(a) => a,
            StructuralDiff::Change(a, b) => a,
            StructuralDiff::Inherited(a) => Sh::map(self.clone(), a),
        }
    }
    
    fn apply_vec<Sh: Shaped>(&self, x: <Morphism as Shaper>::Vec<Sh::Result<Morphism>>) -> <Point as Shaper>::Vec<Sh::Result<Point>> {
        x.into_iter().flat_map(|item| match item {
            ItemDiff::Removed(a) => Some(a),
            ItemDiff::Added(_) => None,
            ItemDiff::Change(a, _) => Some(a),
            ItemDiff::Inherited(a) => Some(Sh::map(self.clone(), a)),
            ItemDiff::Unchanged(a) => Some(a),
        }).collect()
    }
    
    fn apply_disc<A>(&self, x: <Morphism as Shaper>::Disc<A>) -> <Point as Shaper>::Disc<A> {
        match x {
            DiscDiff::Same(a) => a,
            DiscDiff::Changed(a, _) => a,
        }
    }
}

