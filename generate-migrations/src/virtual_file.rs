use std::{collections::BTreeMap, fmt::Display, path::PathBuf, result};

use tokio::fs::write;

use crate::typediff::{DiscreteMorphism, EnumVariant, Point, StructField, TupleEntry, TypeName, TypeRepr};
use std::fs;

pub struct VirtualFile {
	// condition: all typereprs should be flat!
	definitions: BTreeMap<TypeName, TypeRepr<Point>>,

	path: PathBuf,
}

impl VirtualFile {
	pub fn new(path: PathBuf) -> Self {
		Self { definitions: Default::default(), path }
	}

    pub fn write_struct_field(&mut self, f: StructField<Point>) -> StructField<Point> {
        StructField { name: f.name, position: f.position, ty: self.write(f.ty) }
    }

    pub fn write_enum_variant(&mut self, f: EnumVariant<Point>) -> EnumVariant<Point> {
        EnumVariant { variant: f.variant, fields: f.fields.into_iter().map(|field| self.write_struct_field(field)).collect() }
    }

    pub fn write_tuple_entry(&mut self, f: TupleEntry<Point>) -> TupleEntry<Point> {
        TupleEntry { position: f.position, ty: self.write(f.ty) }
    }

	pub fn write(&mut self, def: TypeRepr<Point>) -> TypeRepr<Point> {
		use TypeRepr::*;
			// Struct { typename, .. } |
			// Enum { typename, .. } |
			// Tuple { typename, .. } |
			// Sequence { typename, .. } => {
			// 	if !self.definitions.contains_key(&typename) {
			// 		self.definitions.insert(typename.clone(), a.clone());
			// 	}
			// 	return TypeRepr::TypeByName(typename.clone())
			// },
		match &def {
                a @ Primitive(_) => a.clone(),
                a @ TypeByName(discrete_morphism) => a.clone(),
                a @ NotImplemented => todo!(),
                Struct { typename, fields } => {
                    let fields = fields.into_iter().map(|field| self.write_struct_field(field.clone())).collect();
                    let result = TypeRepr::Struct { typename: typename.clone(), fields };
                    self.definitions.insert(typename.clone(), result.clone());
                    TypeRepr::TypeByName(typename.clone())
                },
                Enum { typename, variants } => {
                    let variants = variants.into_iter().map(|field| self.write_enum_variant(field.clone())).collect();
                    let result = TypeRepr::Enum { typename: typename.clone(), variants };
                    self.definitions.insert(typename.clone(), result.clone());
                    TypeRepr::TypeByName(typename.clone())
                },
                Tuple { typename, fields } => {
                    let fields = fields.into_iter().map(|field| self.write_tuple_entry(field.clone())).collect();
                    let result = TypeRepr::Tuple { typename: typename.clone(), fields };
                    result
                },
                Sequence { typename, inner } => {
                    let inner = self.write(*inner.clone());
                    let result = TypeRepr::Sequence { typename: typename.clone(), inner: Box::new(inner) };
                    result
                },
            }
	}

	pub fn apply(&self) {
		// create directories
		fs::create_dir_all(self.path.parent().unwrap()).unwrap();

		// create file
		let file = fs::File::create(self.path.clone()).unwrap();

		for (_, repr) in &self.definitions {
			println!("writing {repr}");
		}
	}
}

impl Display for TypeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeName::Ordinary { path, chain, params } => {
                write!(f, "{}", path)?;
                if params.len() > 0 {
                    let x = params.iter().map(|x| x.to_string()).intersperse(", ".to_string()).collect::<Vec<_>>().concat();
                    write!(f, "<{x}>")?;
                }
                Ok(())
            },
            TypeName::VariantType { enum_type, variant } => todo!(),
            TypeName::Unknown => write!(f, "?"),
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
        write!(f, "{} {{", self.variant)?;
        for field in &self.fields {
            write!(f, "{}", field)?;
        }
        write!(f, "}}")
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
			TypeRepr::Tuple { typename, fields } => (),
			TypeRepr::Sequence { typename, inner } => (),
			TypeRepr::NotImplemented => (),
			TypeRepr::Primitive(_) => (),
			TypeRepr::TypeByName(discrete_morphism) => write!(f, "{}", discrete_morphism)?,
		}
        Ok(())
	}
}
