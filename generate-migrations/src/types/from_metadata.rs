use crate::types::definition::{EnumVariant, Point, StructField, TypeExpr};

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
		Variant(type_def_variant) => TypeExpr::Enum {
			variants: type_def_variant
				.variants
				.clone()
				.into_iter()
				.map(|variant| EnumVariant {
					pos: variant.index as usize,
					name: variant.name,
					fields: variant
						.fields
						.into_iter()
						.enumerate()
						.map(|(pos, field)| StructField {
							pos,
							name: field.name,
							ty: extract_type(metadata, field.ty.id),
						})
						.collect(),
				})
				.collect(),
		},
		Sequence(type_def_sequence) => TypeExpr::VecLike {
			inner: Box::new(extract_type(metadata, type_def_sequence.type_param.id)),
		},
		Array(type_def_array) => TypeExpr::NotImplemented,
		Tuple(type_def_tuple) => TypeExpr::Tuple {
			entries: type_def_tuple
				.fields
				.clone()
				.into_iter()
				.map(|field| extract_type(metadata, field.id))
				.collect(),
		},
		Primitive(type_def_primitive) => TypeExpr::Primitive { prim: type_def_primitive.clone() },
		Compact(type_def_compact) => TypeExpr::NotImplemented,
		BitSequence(type_def_bit_sequence) => TypeExpr::NotImplemented,
	}
}
