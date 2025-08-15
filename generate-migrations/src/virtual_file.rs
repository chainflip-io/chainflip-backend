use std::{collections::BTreeMap, fmt::Display, io::Write, path::PathBuf, result};

use tokio::fs::write;

use crate::typediff::{
	DiscreteMorphism, EnumVariant, Point, StructField, TupleEntry, TypeName, TypeRepr,
	primitive_to_string, type_repr_to_name,
};
use std::fs;

pub struct VirtualFile {
	path: PathBuf,
	contents: String,
}

impl VirtualFile {
	pub fn apply(&self) {
		let mut path = self.path.clone();
		path.add_extension("rs");

		println!("writing file {}", path.display());

		// create directories
		fs::create_dir_all(path.parent().unwrap()).unwrap();

		// create file
		let mut file = fs::File::create(path.clone()).unwrap();

		writeln!(file, "{}", self.contents).unwrap();
	}
}

pub struct Module {
	// condition: all typereprs should be flat!
	definitions: BTreeMap<TypeName, TypeRepr<Point>>,

	path: PathBuf,

	sub_modules: BTreeMap<String, Module>,
}

impl Module {
	pub fn new(path: PathBuf) -> Self {
		Self { definitions: Default::default(), path, sub_modules: Default::default() }
	}

	pub fn write_struct_field(&mut self, f: StructField<Point>) -> StructField<Point> {
		StructField { name: f.name, position: f.position, ty: self.write(f.ty) }
	}

	pub fn write_enum_variant(&mut self, f: EnumVariant<Point>) -> EnumVariant<Point> {
		EnumVariant {
			variant: f.variant,
			fields: f.fields.into_iter().map(|field| self.write_struct_field(field)).collect(),
		}
	}

	pub fn write_tuple_entry(&mut self, f: TupleEntry<Point>) -> TupleEntry<Point> {
		TupleEntry { position: f.position, ty: self.write(f.ty) }
	}

	/// Condition: name should not contain multiple modules
	fn add_definition_this_module(&mut self, name: TypeName, ty: TypeRepr<Point>) {
		if let Some(old) = self.definitions.get(&name) {
			if *old != ty {
				println!(
					"WARNING: name {name} got new different typerepr. \nold: {old}\n, new: {ty}"
				);
			}
		}

		// clear path of typename & generic params
		self.definitions.insert(
			name,
			ty.map_definition_typename(|name| match name {
				TypeName::Ordinary { path, name, chain, params } =>
					TypeName::Ordinary { path: vec![], name, chain, params: vec![] },
				a => a,
			}),
		);
	}

	/// Resolves submodule path of name
	fn add_definition(&mut self, name: TypeName, ty: TypeRepr<Point>) {
		if let Some((module, name)) = name.clone().split_module() {
			self.sub_modules
				.entry(module.clone())
				.or_insert(Module::new(self.path.join(module.clone())))
				.add_definition(name, ty);
		} else {
			self.add_definition_this_module(name, ty);
		}
	}

	pub fn write(&mut self, def: TypeRepr<Point>) -> TypeRepr<Point> {
		let add_old_path = |n: TypeName| {
			n.map_path(|path| {
				if path.is_empty() {
					// we don't add anything if the typename has no path (this probably means that
					// it's a builtin, like BTreeMap)
					path
				} else {
					["crate", "generated", "old"]
						.into_iter()
						.map(|a| a.to_string())
						.chain(path.into_iter())
						.collect()
				}
			})
		};

		use TypeRepr::*;
		match &def {
			a @ Primitive(_) => a.clone(),
			a @ TypeByName(discrete_morphism) => a.clone(),
			a @ NotImplemented => {
				println!("WARNING, writing notimplemented typerepr");
				a.clone()
			},
			ref a @ Struct { typename, fields } => {
				let fields = fields
					.into_iter()
					.map(|field| self.write_struct_field(field.clone()))
					.collect();

				let result = TypeRepr::Struct { typename: typename.clone(), fields };
				self.add_definition(typename.clone(), result.clone());
				// TypeRepr::TypeByName(add_old_path(typename.clone()))
				TypeRepr::TypeByName(add_old_path(type_repr_to_name(result)))
			},
			Enum { typename, variants } => {
				let variants = variants
					.into_iter()
					.map(|field| self.write_enum_variant(field.clone()))
					.collect();
				let result = TypeRepr::Enum { typename: typename.clone(), variants };
				self.add_definition(typename.clone(), result.clone());
				// TypeRepr::TypeByName(add_old_path(typename.clone()))
				TypeRepr::TypeByName(add_old_path(type_repr_to_name(result)))
			},
			Tuple { typename, fields } => {
				let fields =
					fields.into_iter().map(|field| self.write_tuple_entry(field.clone())).collect();
				let result = TypeRepr::Tuple { typename: typename.clone(), fields };
				result
			},
			Sequence { typename, inner } => {
				let inner = self.write(*inner.clone());
				let result =
					TypeRepr::Sequence { typename: typename.clone(), inner: Box::new(inner) };
				result
			},
		}
	}

	pub fn apply(&self) -> Vec<VirtualFile> {
		let mut contents = String::new();
		let mut files = Vec::new();

		for (name, module) in &self.sub_modules {
			files.append(&mut module.apply());
			contents.push_str(&format!("pub mod {name};\n"));
		}
		contents.push_str("\n");

		for (typename, def) in self.definitions.clone() {
			let prefix;
			let postfix;
			if let TypeName::Ordinary { path, name, chain: Some(chain), params } = typename {
				prefix = format!("mod {chain} {{\n");
				postfix = "}\n".to_string();
			} else {
				prefix = String::new();
				postfix = String::new();
			}

			contents.push_str(&prefix);
			contents.push_str(&def.to_string());
			contents.push_str("\n");
			contents.push_str(&postfix);
		}

		// contents.push_str(&self.definitions.clone().into_values().map(|def|
		// def.to_string()).intersperse("\n".to_string()).collect::<Vec<_>>().concat());

		files.push(VirtualFile { path: self.path.clone(), contents });

		files
	}
}

impl Display for TypeName {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			TypeName::Ordinary { path, name, chain, params } => {
				if path.len() > 0 {
					write!(
						f,
						"{}::",
						path.clone()
							.into_iter()
							.intersperse("::".to_string())
							.collect::<Vec<_>>()
							.concat()
					)?;
				}

				write!(f, "{}", name)?;
				if params.len() > 0 {
					let x = params
						.iter()
						.map(|x| x.to_string())
						.intersperse(", ".to_string())
						.collect::<Vec<_>>()
						.concat();
					write!(f, "<{x}>")?;
				}
				Ok(())
			},
			TypeName::VariantType { enum_type, variant } => todo!(),
			TypeName::Unknown => write!(f, "?"),
			TypeName::Parameter { variable_name: name, value } => {
				// write!(f, "\"{name}\"=")?;
				if let Some(value) = value {
					write!(f, "{value}")?;
				} else {
					write!(f, "?")?;
				}
				Ok(())
			},
			TypeName::Tuple(type_names) => {
				write!(f, "(")?;
				for ty in type_names {
					write!(f, "{ty}, ")?;
				}
				write!(f, ")")
			},
		}
	}
}

impl Display for StructField<Point> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "pub {}: {},", self.name, self.ty)
	}
}

impl Display for EnumVariant<Point> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		writeln!(f, "{} {{", self.variant)?;
		for field in &self.fields {
			writeln!(f, "{}", field)?;
		}
		write!(f, "}}")
	}
}

impl Display for TupleEntry<Point> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.ty)
	}
}

impl Display for TypeRepr<Point> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			TypeRepr::Struct { typename, fields } => {
				writeln!(f, "pub struct {typename} {{ ")?;
				for field in fields {
					writeln!(f, "{}", field)?;
				}
				writeln!(f, "}}")?;
			},
			TypeRepr::Enum { typename, variants } => {
				writeln!(f, "pub enum {typename} {{ ")?;
				for variant in variants {
					writeln!(f, "{}", variant)?;
				}
				writeln!(f, "}}")?;
			},
			TypeRepr::Tuple { typename, fields } => {
				write!(f, "(")?;
				for field in fields {
					write!(f, "{field}, ")?;
				}
				write!(f, ")")?;
			},
			TypeRepr::Sequence { typename, inner } => {
				write!(f, "Vec<{inner}>")?;
			},
			TypeRepr::NotImplemented => (),
			TypeRepr::Primitive(a) => {
				let name = primitive_to_string(a);
				write!(f, "{name}")?
			},
			TypeRepr::TypeByName(discrete_morphism) => write!(f, "{}", discrete_morphism)?,
		}
		Ok(())
	}
}
