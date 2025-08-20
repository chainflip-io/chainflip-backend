use std::collections::BTreeMap;

use subxt::metadata::types::PalletMetadata;
use scale_info::Type;
use scale_info::form::PortableForm;

use crate::types::definition::{
	EnumVariant, PalletStorage, Point, StorageEntry, StructField, TypeExpr, TypeName,
};

pub fn extract_typename(ty: &Type<PortableForm>) -> TypeName {
	TypeName { 
		path: ty.path.namespace().to_vec(), 
		name: ty.path.ident().unwrap_or("BUILTIN".to_string()), 
		has_params: !ty.type_params.is_empty()
	 }
}

pub fn extract_type(metadata: &subxt::Metadata, ty_id: u32) -> TypeExpr<Point> {
	let ty = metadata.types().resolve(ty_id).unwrap();
	use scale_info::TypeDef::*;
	match &ty.type_def {
		Composite(type_def_composite) => 
			TypeExpr::Struct {
				name: extract_typename(ty),
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
		Variant(type_def_variant) => 
			TypeExpr::Enum {
				name: extract_typename(ty),
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

pub fn extract_pallet(
	current_metadata: &subxt::Metadata,
	pallet: &PalletMetadata,
) -> PalletStorage<Point> {
	let mut entries = BTreeMap::new();

	for item in pallet.storage().unwrap().entries() {
		use subxt::metadata::types::StorageEntryType;
		match item.entry_type() {
			StorageEntryType::Plain(value_ty) => {
				let val = extract_type(&current_metadata, *value_ty);
				entries.insert(item.name().to_string(), StorageEntry::Value(val));
			},
			StorageEntryType::Map { hashers, key_ty, value_ty } => {
				let key = extract_type(&current_metadata, *key_ty);
				let val = extract_type(&current_metadata, *value_ty);
				entries.insert(item.name().to_string(), StorageEntry::Map(key, val));
			},
		}
	}

	PalletStorage { entries }
}
