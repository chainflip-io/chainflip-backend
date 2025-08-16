

// pub trait CommonTraits = Clone + Debug;

pub fn map_vec<X,Y>(xs: Vec<X>, f: impl Fn(X) -> Y) -> Vec<Y> {
    todo!()
}

pub fn map_second<X,Y,Z>(_: (X,Y), f: impl Fn(Y) -> Z) -> (X,Z) {
    todo!()
}

#[n_functor::derive_n_functor]
pub enum TypeExpr<Type, StructFields, PrimitiveInner> {
    Struct {
        #[map_with(|x: Vec<_>, f| map_vec(x, f))]
        fields: StructField
    },
    Primitive{
        ty: PrimitiveInner
    }
}

struct Single<A>(A);

struct TypeExprOf<S: ShapeAB>(
    TypeExpr<
        // Type =
        S::Item<
            TypeExprOf<S::ShapeA>,
            TypeExprOf<S::ShapeB>,
        >,
        // StructField =
        S::Vec<
            (String, TypeExprOf<S::ShapeA>),
            (String, TypeExprOf<S::ShapeB>)
        >,
        u8
    >
);

pub trait ShapeHom<X: ShapeAB, Y: ShapeAB> {
    fn map<A,B>(&self, c: X::Item<A,B>) -> Y::Item<A,B>;
    fn map_vec<A,B>(&self, c: X::Vec<A,B>) -> Y::Vec<A,B>;
    fn inner_a(&self) -> impl ShapeHom<X::ShapeA, Y::ShapeA>;
    fn inner_b(&self) -> impl ShapeHom<X::ShapeB, Y::ShapeB>;
}

impl<X: ShapeAB> TypeExprOf<X> {
    pub fn map<Y: ShapeAB>(self, f: impl ShapeHom<X,Y>) -> TypeExprOf<Y> {
        let zz: TypeExpr<Y::Item<TypeExprOf<Y::ShapeA>, TypeExprOf<Y::ShapeB>>, u8> =
        self.0.map(
            |ty| Y::map_create(f.map(ty), |a| a.map(f.inner_a()), |b| b.map(f.inner_b())), 
            |ty| Y::map_create(f.map_vec(ty), |a| a.map(f.inner_a()), |b| b.map(f.inner_b())), 
            |x| x
        );
        TypeExprOf(zz)
    }
}

#[derive(Clone)]
struct Id;
impl<X: ShapeAB> ShapeHom<X,X> for Id {
    fn map<A,B>(&self, c: <X as ShapeAB>::Item<A,B>) -> <X as ShapeAB>::Item<A,B> {
        c
    }
    
    fn map_vec<A,B>(&self, c: <X as ShapeAB>::Vec<A,B>) -> Optional<<X as ShapeAB>::Vec<A,B>> {
        todo!()
    }

    fn inner_a(&self) -> impl ShapeHom<<X as ShapeAB>::ShapeA, <X as ShapeAB>::ShapeA> {
        Id
    }

    fn inner_b(&self) -> impl ShapeHom<<X as ShapeAB>::ShapeB, <X as ShapeAB>::ShapeB> {
        Id
    }
}


// ------- concrete -------

struct Point;
struct Morphism;


#[derive(Clone, PartialEq, Debug)]
#[n_functor::derive_n_functor]
pub enum ItemDiff<Point, Morphism> {
	Removed(Point),
	Added(Point),
	Change(Point, Point),
	Inherited(Morphism),
	Unchanged(Point),
}

#[derive(Clone, PartialEq, Debug)]
#[n_functor::derive_n_functor]
pub enum StructuralDiff<Point,Morphism> {
    Unchanged(Point),
	Change(Point, Point),
    Inherited(Morphism)
}



trait ShapeAB {
    type Item<A,B>;
    type ShapeA: ShapeAB;
    type ShapeB: ShapeAB;
    type Vec<A,B>;
    fn map_create<A0, A1, B0, B1>(x: Self::Item<A0,B0>, f: impl Fn(A0) -> A1, g: impl Fn(B0) -> B1) -> Self::Item<A1, B1>;
}

impl ShapeAB for Point {
    type Item<A,B> = A;
    type ShapeA = Point;
    type ShapeB = Point;
    type Vec<A,B> = Vec<A>;
    
    fn map_create<A0, A1, B0, B1>(x: Self::Item<A0,B0>, f: impl Fn(A0) -> A1, g: impl Fn(B0) -> B1) -> Self::Item<A1, B1> {
        f(x)
    }
}

impl ShapeAB for Morphism {
    type Item<A,B> = StructuralDiff<A,B>;
    type ShapeA = Point;
    type ShapeB = Morphism;
    type Vec<A,B> = Vec<ItemDiff<A,B>>;
    
    fn map_create<A0, A1, B0, B1>(x: Self::Item<A0,B0>, f: impl Fn(A0) -> A1, g: impl Fn(B0) -> B1) -> Self::Item<A1, B1> {
        x.map(f, g)
    }
}


//----- projecting to one type -----
#[derive(Clone)]
pub enum ProjectPoint {
    Old, New
}

impl ShapeHom<Morphism, Point> for ProjectPoint {
    fn map<A,B>(&self, c: <Morphism as ShapeAB>::Item<A,B>) -> <Point as ShapeAB>::Item<A,B> {
        match (self, c) {
            (ProjectPoint::Old, StructuralDiff::Unchanged(c)) => c,
            (ProjectPoint::Old, StructuralDiff::Change(a, _)) => a,
            (ProjectPoint::Old, StructuralDiff::Inherited(c)) => todo!(),
            (ProjectPoint::New, StructuralDiff::Unchanged(_)) => todo!(),
            (ProjectPoint::New, StructuralDiff::Change(_, _)) => todo!(),
            (ProjectPoint::New, StructuralDiff::Inherited(_)) => todo!(),
        }
    }

    fn inner_a(&self) -> impl ShapeHom<<Morphism as ShapeAB>::ShapeA, <Point as ShapeAB>::ShapeA> {
        Id
    }

    fn inner_b(&self) -> impl ShapeHom<<Morphism as ShapeAB>::ShapeB, <Point as ShapeAB>::ShapeB> {
        self.clone()
    }
    
    fn map_vec<A,B>(&self, c: <Morphism as ShapeAB>::Vec<A,B>) -> <Point as ShapeAB>::Vec<A,B> {
        todo!()
    }
}

