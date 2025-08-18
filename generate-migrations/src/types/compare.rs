use core::panic;
use std::collections::{BTreeMap, BTreeSet};

use crate::types::definition::{ItemDiff, Morphism, PalletStorage, Point, Shape, Shaper, StructuralDiff, TypeExpr};

pub fn diff_typeexpr(x: &TypeExpr<Point>, y: &TypeExpr<Point>) -> <Morphism as Shaper>::Strl<TypeExpr<Morphism>> {
	match (x, y) {
		(TypeExpr::Struct { fields: fields1 }, TypeExpr::Struct { fields: fields2 }) => todo!(),
		(TypeExpr::Enum { variants: v1 }, TypeExpr::Enum { variants: v2 }) => todo!(),
		(TypeExpr::VecLike { inner: i1 }, TypeExpr::VecLike { inner: i2 }) => todo!(),
		(TypeExpr::MapLike { key: key1, val: val1 }, TypeExpr::MapLike { key: key2, val: val2 }) => todo!(),
		(TypeExpr::Tuple { entries: entries1 }, TypeExpr::Tuple { entries: entries2 }) => {
            StructuralDiff::Inherited(TypeExpr::Tuple { entries: compute_diff_by(entries1, entries2, |entry| entry.clone(), diff_typeexpr) })
        },
		(TypeExpr::Primitive { prim: prim1 }, TypeExpr::Primitive { prim: prim2 }) => {
            if prim1 == prim2 {
                StructuralDiff::Unchanged(TypeExpr::Primitive { prim: prim1.clone() })
            } else {
                StructuralDiff::Change(x.clone(), y.clone())
            }
        },
		(TypeExpr::NotImplemented, TypeExpr::NotImplemented) => todo!(),
        (x, y) => StructuralDiff::Change(x.clone(), y.clone())
	}
}

pub fn diff_pallet_storage(x: &PalletStorage<Point>, y: PalletStorage<Point>) -> StructuralDiff<PalletStorage<Morphism>> {
    compute_diff_by(x.entries, y.entries, get_name, inner_diff)
}

pub fn compute_diff_by<A: Shape, Name: Ord>(xs: impl IntoIterator<&A::Next>, ys: impl IntoIterator<&A::Next>, get_name: impl Fn(&A::Next) -> Name, inner_diff: impl Fn(&A::Next,&A::Next) -> StructuralDiff<A>) -> Vec<ItemDiff<A>> {
    let x_by_name: BTreeMap<Name, &A::Next> = xs.into_iter().map(|x| (get_name(x), x)).collect();
    let y_by_name: BTreeMap<Name, &A::Next> = ys.into_iter().map(|y| (get_name(y), y)).collect();
    let x_names: BTreeSet<_> = x_by_name.keys().collect();
    let y_names: BTreeSet<_> = y_by_name.keys().collect();
    let mut result: BTreeSet<ItemDiff<A>> = Default::default();
    for name in x_names.union(&y_names) {
        match (x_by_name.get(name).cloned(), y_by_name.get(name).cloned()) {
            (None, None) => panic!(),
            (None, Some(a)) => result.insert(ItemDiff::Added(a.clone())),
            (Some(a), None) => result.insert(ItemDiff::Removed(a.clone())),
            (Some(a), Some(b)) => match inner_diff(a,b) {
                StructuralDiff::Unchanged(a) => result.insert(ItemDiff::Unchanged(a)),
                StructuralDiff::Change(a, b) => result.insert(ItemDiff::Change(a, b)),
                StructuralDiff::Inherited(f) => result.insert(ItemDiff::Inherited(f)),
            },
        };
    }
    result.into_iter().collect()
}
