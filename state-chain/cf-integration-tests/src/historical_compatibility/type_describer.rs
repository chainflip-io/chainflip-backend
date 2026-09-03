// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use frame_metadata::v15::RuntimeMetadataV15;
use scale_info::{
	form::PortableForm, MetaType, PortableRegistry, Registry, Type, TypeDef, TypeDefPrimitive,
	TypeInfo,
};

pub fn describe_expected_type<T: TypeInfo + 'static>() -> String {
	let mut registry = Registry::new();
	let type_id = registry.register_type(&MetaType::new::<T>()).id;
	let portable_registry = PortableRegistry::from(registry);
	describe_type(&portable_registry, type_id)
}

pub fn describe_metadata_type(metadata: &RuntimeMetadataV15, type_id: u32) -> String {
	describe_type(&metadata.types, type_id)
}

/// Describe multiple metadata types as if they were a tuple.
///
/// This is used for runtime API inputs where metadata stores each parameter as a separate
/// type rather than as a single tuple type.
pub fn describe_metadata_types_as_tuple(metadata: &RuntimeMetadataV15, type_ids: &[u32]) -> String {
	let registry = &metadata.types;
	let mut lines = Vec::new();
	let mut path = Vec::new();
	lines.push("<anonymous>".to_string());
	for (index, &type_id) in type_ids.iter().enumerate() {
		push_named_child(registry, format!("[{index}]"), type_id, 2, &mut path, &mut lines);
	}
	let mut result = lines.join("\n");
	result.push('\n');
	result
}

pub fn metadata_type_name(metadata: &RuntimeMetadataV15, type_id: u32) -> Option<String> {
	get_type_name(&metadata.types, type_id)
}

fn describe_type(registry: &PortableRegistry, type_id: u32) -> String {
	let mut lines = Vec::new();
	let mut path = Vec::new();
	let portable_type = &registry.types[type_id as usize].ty;
	lines.push(type_path(portable_type));
	push_type_body(registry, type_id, 2, &mut path, &mut lines);
	let mut result = lines.join("\n");
	result.push('\n');
	result
}

fn get_type_name(registry: &PortableRegistry, type_id: u32) -> Option<String> {
	let portable_type = &registry.types[type_id as usize].ty;
	let name = type_path(portable_type);
	if name == "<anonymous>" {
		None
	} else {
		Some(name)
	}
}

fn is_unit_type(registry: &PortableRegistry, type_id: u32) -> bool {
	matches!(
		&registry.types[type_id as usize].ty.type_def,
		TypeDef::Tuple(tuple) if tuple.fields.is_empty()
	)
}

fn push_type_body(
	registry: &PortableRegistry,
	type_id: u32,
	indent: usize,
	path: &mut Vec<u32>,
	lines: &mut Vec<String>,
) {
	let prefix = " ".repeat(indent);
	let portable_type = &registry.types[type_id as usize].ty;

	if path.contains(&type_id) {
		return;
	}

	path.push(type_id);

	match &portable_type.type_def {
		TypeDef::Composite(composite) =>
			for field in &composite.fields {
				if is_unit_type(registry, field.ty.id) {
					continue;
				}
				let field_name = field.name.as_deref().unwrap_or("<unnamed>");
				push_named_child(
					registry,
					format!(".{field_name}"),
					field.ty.id,
					indent,
					path,
					lines,
				)
			},
		TypeDef::Variant(variant) =>
			for entry in &variant.variants {
				lines.push(format!("{prefix}[{}] {}", entry.index, entry.name));
				for field in &entry.fields {
					let field_name = field.name.as_deref().unwrap_or("<unnamed>");
					push_named_child(
						registry,
						format!(".{field_name}"),
						field.ty.id,
						indent + 2,
						path,
						lines,
					)
				}
			},
		TypeDef::Sequence(sequence) => {
			let inner_type_id = sequence.type_param.id;
			let inner_type = &registry.types[inner_type_id as usize].ty;
			lines.push(format!("{prefix}[seq] -> {}", type_path(inner_type)));
			if should_expand(inner_type) {
				push_type_body(registry, inner_type_id, indent + 2, path, lines);
			}
		},
		TypeDef::Array(array) => {
			let inner_type_id = array.type_param.id;
			let inner_type = &registry.types[inner_type_id as usize].ty;
			lines.push(format!("{prefix}[array; {}] -> {}", array.len, type_path(inner_type),));
			if should_expand(inner_type) {
				push_type_body(registry, inner_type_id, indent + 2, path, lines);
			}
		},
		TypeDef::Tuple(tuple) =>
			for (index, field_type) in tuple.fields.iter().enumerate() {
				push_named_child(registry, format!("[{index}]"), field_type.id, indent, path, lines)
			},
		TypeDef::Primitive(_) | TypeDef::Compact(_) | TypeDef::BitSequence(_) => {},
	}

	path.pop();
}

fn push_named_child(
	registry: &PortableRegistry,
	label: String,
	type_id: u32,
	indent: usize,
	path: &mut Vec<u32>,
	lines: &mut Vec<String>,
) {
	let child_type = &registry.types[type_id as usize].ty;
	match transparent_wrapper_kind(child_type) {
		Some(TransparentWrapperKind::GeneratedEnumVariantPayload) => {
			push_generated_enum_variant_payload(registry, &label, type_id, indent, path, lines);
			return;
		},
		None => {},
	}

	let prefix = " ".repeat(indent);
	lines.push(format!("{prefix}{label}: {}", type_path(child_type)));
	if should_expand(child_type) {
		push_type_body(registry, type_id, indent + 2, path, lines);
	}
}

enum TransparentWrapperKind {
	GeneratedEnumVariantPayload,
}

fn transparent_wrapper_kind(ty: &Type<PortableForm>) -> Option<TransparentWrapperKind> {
	if matches!(ty.type_def, TypeDef::Composite(_)) &&
		is_generated_enum_variant_payload_path(&ty.path.segments)
	{
		Some(TransparentWrapperKind::GeneratedEnumVariantPayload)
	} else {
		None
	}
}

fn is_generated_enum_variant_payload_path(segments: &[String]) -> bool {
	segments.len() >= 5 &&
		segments[segments.len() - 5] == "variants" &&
		segments[segments.len() - 4] == "__impls" &&
		segments[segments.len() - 2] == "variant_mod" &&
		segments[segments.len() - 1] == "Struct"
}

fn push_generated_enum_variant_payload(
	registry: &PortableRegistry,
	label: &str,
	type_id: u32,
	indent: usize,
	path: &mut Vec<u32>,
	lines: &mut Vec<String>,
) {
	if path.contains(&type_id) {
		return;
	}

	path.push(type_id);

	if let TypeDef::Composite(composite) = &registry.types[type_id as usize].ty.type_def {
		let tuple_fields =
			generated_tuple_fields(composite.fields.iter().map(|field| field.name.as_deref()));
		for field in &composite.fields {
			if is_unit_type(registry, field.ty.id) {
				continue;
			}
			let field_label = match field.name.as_deref() {
				Some(field_name) if !tuple_fields => format!(".{field_name}"),
				_ => label.to_string(),
			};
			push_named_child(registry, field_label, field.ty.id, indent, path, lines);
		}
	}

	path.pop();
}

fn generated_tuple_fields<'a>(field_names: impl Iterator<Item = Option<&'a str>>) -> bool {
	field_names
		.enumerate()
		.all(|(index, field_name)| field_name == Some(format!("_{index}").as_str()))
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
	match &ty.type_def {
		TypeDef::Primitive(primitive) => primitive_name(primitive).to_string(),
		_ if ty.path.segments.is_empty() => "<anonymous>".to_string(),
		_ => replace_type_name(ty.path.segments.join("::")),
	}
}

fn replace_type_name(type_name: String) -> String {
	let mut segments: Vec<_> = type_name.split("::").collect();

	if segments.len() >= 2 {
		let wrapper_index = segments.len() - 2;
		if matches!(segments[segments.len() - 1], "Enum" | "Struct") &&
			segments[wrapper_index].starts_with('_')
		{
			segments[wrapper_index] = &segments[wrapper_index][1..];
			segments.pop();
			return segments.join("::");
		}
	}

	type_name
}

fn primitive_name(primitive: &TypeDefPrimitive) -> &'static str {
	match primitive {
		TypeDefPrimitive::Bool => "bool",
		TypeDefPrimitive::Char => "char",
		TypeDefPrimitive::Str => "str",
		TypeDefPrimitive::U8 => "u8",
		TypeDefPrimitive::U16 => "u16",
		TypeDefPrimitive::U32 => "u32",
		TypeDefPrimitive::U64 => "u64",
		TypeDefPrimitive::U128 => "u128",
		TypeDefPrimitive::U256 => "u256",
		TypeDefPrimitive::I8 => "i8",
		TypeDefPrimitive::I16 => "i16",
		TypeDefPrimitive::I32 => "i32",
		TypeDefPrimitive::I64 => "i64",
		TypeDefPrimitive::I128 => "i128",
		TypeDefPrimitive::I256 => "i256",
	}
}
