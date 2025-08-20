use core::panic;
use std::collections::{BTreeMap, BTreeSet};

use crate::types::definition::{
	DiscDiff, EnumVariant, ItemDiff, Morphism, PalletStorage, Point, Shape, Shaper, StorageEntry,
	StructField, StructuralDiff, TypeExpr, TypeName,
};

pub fn diff_disc<X: PartialEq + Clone>(x: &X, y: &X) -> DiscDiff<X> {
	if x == y { DiscDiff::Same(x.clone()) } else { DiscDiff::Changed(x.clone(), y.clone()) }
}

pub fn diff_struct_field(
	x: &StructField<Point>,
	y: &StructField<Point>,
) -> StructuralDiff<StructField<Morphism>> {
	if x == y {
		StructuralDiff::Unchanged(x.clone())
	} else {
		let pos = diff_disc(&x.pos, &y.pos);
		let name = diff_disc(&x.name, &y.name);
		let ty = diff_typeexpr(&x.ty, &y.ty);
		StructuralDiff::Inherited(StructField { pos, name, ty })
	}
}

pub fn diff_enum_variant(
	x: &EnumVariant<Point>,
	y: &EnumVariant<Point>,
) -> StructuralDiff<EnumVariant<Morphism>> {
	if x == y {
		StructuralDiff::Unchanged(x.clone())
	} else {
		let pos = diff_disc(&x.pos, &y.pos);
		let name = diff_disc(&x.name, &y.name);
		let fields1 = x.fields.clone().into_iter().map(|f| (f.name.clone(), f));
		let fields2 = y.fields.clone().into_iter().map(|f| (f.name.clone(), f));
		let fields = diff_item(fields1, fields2, diff_struct_field);
		StructuralDiff::Inherited(EnumVariant { pos, name, fields })
	}
}

pub fn diff_typeexpr(
	x: &TypeExpr<Point>,
	y: &TypeExpr<Point>,
) -> <Morphism as Shaper>::Strl<TypeExpr<Morphism>> {
	match (x, y) {
		(
			TypeExpr::Struct { name: name1, fields: fields1 },
			TypeExpr::Struct { name: name2, fields: fields2 },
		) => {
            let name = diff_disc(name1, name2);
            match name {
                DiscDiff::Same(name) if !name.has_params && fields1 == fields2 =>
                    StructuralDiff::Unchanged(TypeExpr::ByName(name)),
                _ => {
                    let fields1 = fields1.clone().into_iter().map(|f| (f.name.clone(), f));
                    let fields2 = fields2.clone().into_iter().map(|f| (f.name.clone(), f));
                    StructuralDiff::Inherited(TypeExpr::Struct {
                        name,
                        fields: diff_item(fields1, fields2, diff_struct_field),
                    })
                }
            }

		},
		(
			TypeExpr::Enum { name: name1, variants: v1 },
			TypeExpr::Enum { name: name2, variants: v2 },
		) => {
            let name = diff_disc(name1, name2);
            match name {
                DiscDiff::Same(name) if !name.has_params && v1 == v2 =>
                    StructuralDiff::Unchanged(TypeExpr::ByName(name)),
                _ => {
                    let v1 = v1.clone().into_iter().map(|f| (f.name.clone(), f));
                    let v2 = v2.clone().into_iter().map(|f| (f.name.clone(), f));
                    StructuralDiff::Inherited(TypeExpr::Enum {
                        name,
                        variants: diff_item(v1, v2, diff_enum_variant),
                    })

                }
            }
		},
		(TypeExpr::VecLike { inner: i1 }, TypeExpr::VecLike { inner: i2 }) =>
			StructuralDiff::Inherited(TypeExpr::VecLike { inner: Box::new(diff_typeexpr(i1, i2)) }),
		(
			TypeExpr::MapLike { key: key1, val: val1 },
			TypeExpr::MapLike { key: key2, val: val2 },
		) => StructuralDiff::Inherited(TypeExpr::MapLike {
			key: Box::new(diff_typeexpr(key1, key2)),
			val: Box::new(diff_typeexpr(val1, val2)),
		}),
		(TypeExpr::Tuple { entries: entries1 }, TypeExpr::Tuple { entries: entries2 }) => {
			let entries1 = entries1.clone().into_iter().enumerate();
			let entries2 = entries2.clone().into_iter().enumerate();
			StructuralDiff::Inherited(TypeExpr::Tuple {
				entries: diff_item(entries1, entries2, diff_typeexpr),
			})
		},
		(TypeExpr::Primitive { prim: prim1 }, TypeExpr::Primitive { prim: prim2 }) => {
			if prim1 == prim2 {
				StructuralDiff::Unchanged(TypeExpr::Primitive { prim: prim1.clone() })
			} else {
				StructuralDiff::Change(x.clone(), y.clone())
			}
		},
		(TypeExpr::NotImplemented, TypeExpr::NotImplemented) => todo!(),
		(x, y) => StructuralDiff::Change(x.clone(), y.clone()),
	}
}

pub fn diff_storage_entry(
	x: &StorageEntry<Point>,
	y: &StorageEntry<Point>,
) -> StructuralDiff<StorageEntry<Morphism>> {
	match (x, y) {
		(StorageEntry::Value(x), StorageEntry::Value(y)) =>
			StructuralDiff::Inherited(StorageEntry::Value(diff_typeexpr(x, y))),
		(StorageEntry::Map(k1, v1), StorageEntry::Map(k2, v2)) => StructuralDiff::Inherited(
			StorageEntry::Map(diff_typeexpr(k1, k2), diff_typeexpr(v1, v2)),
		),
		(x, y) => StructuralDiff::Change(x.clone(), y.clone()),
	}
}

pub fn diff_pallet_storage(
	x: &PalletStorage<Point>,
	y: &PalletStorage<Point>,
) -> StructuralDiff<PalletStorage<Morphism>> {
	let e1 = x.entries.clone();
	let e2 = y.entries.clone();
	StructuralDiff::Inherited(PalletStorage {
		entries: diff_item_and_name(e1, e2, diff_storage_entry),
	})
}

pub fn diff_item<A: Shape, Name: Ord + Clone>(
	xs: impl IntoIterator<Item = (Name, A::Next)>,
	ys: impl IntoIterator<Item = (Name, A::Next)>,
	inner_diff: impl Fn(&A::Next, &A::Next) -> StructuralDiff<A>,
) -> Vec<ItemDiff<A>> {
	diff_item_and_name(xs, ys, inner_diff).into_values().collect()
}

pub fn diff_item_and_name<A: Shape, Name: Ord + Clone>(
	xs: impl IntoIterator<Item = (Name, A::Next)>,
	ys: impl IntoIterator<Item = (Name, A::Next)>,
	inner_diff: impl Fn(&A::Next, &A::Next) -> StructuralDiff<A>,
) -> BTreeMap<Name, ItemDiff<A>> {
	let x_by_name: BTreeMap<Name, A::Next> = xs.into_iter().collect();
	let y_by_name: BTreeMap<Name, A::Next> = ys.into_iter().collect();
	let x_names: BTreeSet<_> = x_by_name.keys().collect();
	let y_names: BTreeSet<_> = y_by_name.keys().collect();
	let mut result: BTreeMap<Name, ItemDiff<A>> = Default::default();
	for name in x_names.union(&y_names) {
		match (x_by_name.get(name).cloned(), y_by_name.get(name).cloned()) {
			(None, None) => panic!(),
			(None, Some(a)) => result.insert((*name).clone(), ItemDiff::Added(a.clone())),
			(Some(a), None) => result.insert((*name).clone(), ItemDiff::Removed(a.clone())),
			(Some(a), Some(b)) =>
				result.insert((*name).clone(), ItemDiff::Structural(inner_diff(&a, &b))),
		};
	}
	result
}
