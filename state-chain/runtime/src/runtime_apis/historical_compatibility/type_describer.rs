use frame_metadata::v15::RuntimeMetadataV15;
use scale_info::{
	form::PortableForm, MetaType, PortableRegistry, Registry, Type, TypeDef, TypeInfo,
};
use std::collections::BTreeSet;

pub fn describe_expected_type<T: TypeInfo + 'static>() -> String {
	let mut registry = Registry::new();
	let type_id = registry.register_type(&MetaType::new::<T>()).id;
	let portable_registry = PortableRegistry::from(registry);
	describe_type(&portable_registry, type_id)
}

pub fn describe_metadata_type(metadata: &RuntimeMetadataV15, type_id: u32) -> String {
	describe_type(&metadata.types, type_id)
}

fn describe_type(registry: &PortableRegistry, type_id: u32) -> String {
	let mut lines = Vec::new();
	let mut visited = BTreeSet::new();
	push_type_description(registry, type_id, 0, &mut visited, &mut lines);
	lines.join("\n")
}

fn push_type_description(
	registry: &PortableRegistry,
	type_id: u32,
	indent: usize,
	visited: &mut BTreeSet<u32>,
	lines: &mut Vec<String>,
) {
	let prefix = " ".repeat(indent);
	let portable_type = &registry.types[type_id as usize].ty;
	lines.push(format!("{prefix}#{} {}", type_id, type_path(portable_type)));

	if !visited.insert(type_id) {
		lines.push(format!("{prefix}  <already expanded>"));
		return;
	}

	match &portable_type.type_def {
		TypeDef::Composite(composite) =>
			for field in &composite.fields {
				let field_name = field.name.as_deref().unwrap_or("<unnamed>");
				let field_type_id = field.ty.id;
				let field_type = &registry.types[field_type_id as usize].ty;
				lines.push(format!(
					"{prefix}  .{field_name}: #{} {}",
					field_type_id,
					type_path(field_type),
				));
				if should_expand(field_type) {
					push_type_description(registry, field_type_id, indent + 4, visited, lines);
				}
			},
		TypeDef::Variant(variant) =>
			for entry in &variant.variants {
				lines.push(format!("{prefix}  [{}] {}", entry.index, entry.name));
				for field in &entry.fields {
					let field_name = field.name.as_deref().unwrap_or("<unnamed>");
					let field_type_id = field.ty.id;
					let field_type = &registry.types[field_type_id as usize].ty;
					lines.push(format!(
						"{prefix}    .{field_name}: #{} {}",
						field_type_id,
						type_path(field_type),
					));
					if should_expand(field_type) {
						push_type_description(registry, field_type_id, indent + 6, visited, lines);
					}
				}
			},
		TypeDef::Sequence(sequence) => {
			let inner_type_id = sequence.type_param.id;
			let inner_type = &registry.types[inner_type_id as usize].ty;
			lines.push(format!("{prefix}  [seq] -> #{} {}", inner_type_id, type_path(inner_type),));
			if should_expand(inner_type) {
				push_type_description(registry, inner_type_id, indent + 4, visited, lines);
			}
		},
		TypeDef::Array(array) => {
			let inner_type_id = array.type_param.id;
			let inner_type = &registry.types[inner_type_id as usize].ty;
			lines.push(format!(
				"{prefix}  [array; {}] -> #{} {}",
				array.len,
				inner_type_id,
				type_path(inner_type),
			));
			if should_expand(inner_type) {
				push_type_description(registry, inner_type_id, indent + 4, visited, lines);
			}
		},
		TypeDef::Tuple(tuple) =>
			for (index, field_type) in tuple.fields.iter().enumerate() {
				let field_type_id = field_type.id;
				let nested_type = &registry.types[field_type_id as usize].ty;
				lines.push(format!(
					"{prefix}  [{}]: #{} {}",
					index,
					field_type_id,
					type_path(nested_type),
				));
				if should_expand(nested_type) {
					push_type_description(registry, field_type_id, indent + 4, visited, lines);
				}
			},
		TypeDef::Primitive(_) | TypeDef::Compact(_) | TypeDef::BitSequence(_) => {},
	}
}

fn should_expand(ty: &Type<PortableForm>) -> bool {
	matches!(
		ty.type_def,
		TypeDef::Composite(_) |
			TypeDef::Variant(_) |
			TypeDef::Sequence(_) |
			TypeDef::Array(_) |
			TypeDef::Tuple(_)
	)
}

fn type_path(ty: &Type<PortableForm>) -> String {
	let path = if ty.path.segments.is_empty() {
		"<anonymous>".to_string()
	} else {
		ty.path.segments.join("::")
	};

	if ty.type_params.is_empty() {
		path
	} else {
		let params = ty
			.type_params
			.iter()
			.map(|param| {
				param.ty.map(|id| format!("#{}", id.id)).unwrap_or_else(|| param.name.clone())
			})
			.collect::<Vec<_>>()
			.join(", ");
		format!("{}<{}>", path, params)
	}
}
