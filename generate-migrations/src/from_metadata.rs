use crate::types2::{Point, StructField, TypeExpr};

pub fn extract_type(metadata: &subxt::Metadata, ty_id: u32) -> TypeExpr<Point> {
	let ty = metadata.types().resolve(ty_id).unwrap();
	use scale_info::TypeDef::*;
	match &ty.type_def {
		Composite(type_def_composite) => TypeExpr::Struct {
			fields: type_def_composite
				.fields
				.clone()
				.into_iter()
				.enumerate()
				.map(|(pos, field)| StructField {
					pos,
					name: field.name,
					ty: extract_type(metadata, field.ty.id),
				})
				.collect(),
		},
		Variant(type_def_variant) => todo!(),
		Sequence(type_def_sequence) => todo!(),
		Array(type_def_array) => todo!(),
		Tuple(type_def_tuple) => todo!(),
		Primitive(type_def_primitive) => todo!(),
		Compact(type_def_compact) => todo!(),
		BitSequence(type_def_bit_sequence) => todo!(),
	}
}
