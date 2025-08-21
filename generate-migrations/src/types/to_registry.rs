use crate::types::{
	definition::{DiscDiff, Shaper, StructField, TypeExpr, TypeName},
	registry::Registry,
};

pub fn to_registry_structfield<S: Shaper>(
	reg: &mut Registry<DiscDiff<TypeName>, TypeExpr<S>>,
	f: StructField<S>,
) -> StructField<S> {
	todo!()
}

pub fn to_registry_typeexpr<S: Shaper>(
	reg: &mut Registry<DiscDiff<TypeName>, TypeExpr<S>>,
	t: TypeExpr<S>,
) -> TypeExpr<S> {
	use TypeExpr::*;
	match t {
		Struct { name, fields } => {
			reg.insert(name.clone(), Struct {
				name,
				fields: fields.into_iter().map(|f| todo!()).collect(),
			});
		},
		Enum { name, variants } => todo!(),
		VecLike { inner } => todo!(),
		MapLike { key, val } => todo!(),
		Tuple { entries } => todo!(),
		Primitive { prim } => todo!(),
		ByName(type_name) => todo!(),
		NotImplemented => todo!(),
	}
}
