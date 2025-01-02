use clap::Parser;
use serde_json::Value;
use std::{collections::BTreeSet, error::Error, fmt, fs, io, str::FromStr};

#[derive(Debug, Clone)]
struct ApiDocs<'a> {
	methods: Vec<MethodDef<'a>>,
	types: Vec<TypeDoc<'a>>,
}

#[derive(Debug, Clone)]
struct MethodDef<'s> {
	method: String,
	request: TypeDoc<'s>,
	response: TypeDoc<'s>,
}

/// A named item in the schema, usually a field of an object.
///
/// Can be required.
#[derive(Debug, Clone)]
struct FieldDoc<'a> {
	pub name: String,
	required: bool,
	pub type_doc: TypeDoc<'a>,
}

impl FieldDoc<'_> {
	pub fn new<'n>(name: &str, required: bool, schema: &'n Value) -> FieldDoc<'n> {
		FieldDoc { name: name.to_string(), required, type_doc: TypeDoc::new(name, schema) }
	}

	pub fn description(&self) -> Option<String> {
		self.type_doc.description()
	}
}

impl std::fmt::Display for FieldDoc<'_> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(
			f,
			"{}{}: {}",
			self.name,
			if self.required { "[Required]" } else { "[Optional]" },
			self.description().unwrap_or_else(|| format!("{}", self.type_doc.variant()))
		)
	}
}

#[derive(Debug, Clone)]
enum PrimitiveType {
	Number(Option<NumberFormat>),
	Boolean,
	String,
	Null,
}

impl std::fmt::Display for PrimitiveType {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			PrimitiveType::Number(Some(format)) => write!(f, "{}", format),
			PrimitiveType::Number(None) => write!(f, "number"),
			PrimitiveType::Boolean => write!(f, "boolean"),
			PrimitiveType::String => write!(f, "string"),
			PrimitiveType::Null => write!(f, "null"),
		}
	}
}

#[derive(Debug, Clone)]
enum NumberFormat {
	Uint16,
	Uint32,
	Uint64,
}

impl std::fmt::Display for NumberFormat {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			NumberFormat::Uint16 => write!(f, "uint16"),
			NumberFormat::Uint32 => write!(f, "uint32"),
			NumberFormat::Uint64 => write!(f, "uint64"),
		}
	}
}

impl std::str::FromStr for NumberFormat {
	type Err = ();
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"uint16" => Ok(NumberFormat::Uint16),
			"uint32" => Ok(NumberFormat::Uint32),
			"uint64" => Ok(NumberFormat::Uint64),
			_ => Err(()),
		}
	}
}

impl PrimitiveType {
	pub fn example(&self) -> Option<Value> {
		match self {
			PrimitiveType::Number(_) => Some(serde_json::json!(42)),
			PrimitiveType::Boolean => Some(serde_json::json!(true)),
			PrimitiveType::String => Some(serde_json::json!("zabba")),
			PrimitiveType::Null => Some(Value::Null),
		}
	}
}

#[derive(Debug, Clone)]
enum TypeVariant<'a> {
	OneOf(Vec<TypeDoc<'a>>),
	Object(Vec<FieldDoc<'a>>),
	Array(Vec<TypeDoc<'a>>),
	Primitive(PrimitiveType),
	Enum(Vec<String>),
	Ref(TypeRef),
	RawSchema(&'a Value),
	Constant(String),
	Empty,
}

#[derive(Debug, Clone)]
struct TypeRef {
	raw: String,
}

trait Linkable {
	fn link(&self) -> String;
}

impl Linkable for &str {
	fn link(&self) -> String {
		self.to_lowercase().replace(" ", "-").replace("::", "")
	}
}

impl Linkable for WithLink<'_> {
	fn link(&self) -> String {
		self.0.link()
	}
}

struct WithLink<'a>(&'a str);

impl fmt::Display for WithLink<'_> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(f, "[`{}`](#{})", self.0, self.0.link())
	}
}

impl TypeRef {
	pub fn new(raw: String) -> Self {
		TypeRef { raw }
	}

	pub fn pointer(&self) -> String {
		self.raw.replace('#', "/")
	}

	pub fn name(&self) -> String {
		self.raw.split('/').last().map(String::from).expect("Invalid type reference")
	}

	pub fn resolve<'root>(&self, root_schema: &'root Value) -> TypeDoc<'root> {
		TypeDoc::new(
			&self.name(),
			root_schema.pointer(&self.pointer()).unwrap_or_else(|| {
				panic!("Invalid schema reference: {} for schema: {}", self.raw, root_schema)
			}),
		)
	}
}

#[derive(Debug, Clone)]
struct TypeDoc<'a> {
	pub name: String,
	pub schema: &'a Value,
}

/// Named type definition in the schema.
impl<'doc> TypeDoc<'doc> {
	pub fn new<'new>(name: &str, schema: &'new Value) -> TypeDoc<'new> {
		TypeDoc { name: name.to_string(), schema }
	}

	pub fn from_ref(schema: &Value) -> TypeDoc<'_> {
		TypeDoc {
			name: TypeRef::new(
				schema
					.get("$ref")
					.and_then(|r| r.as_str())
					.expect("Invalid type reference")
					.to_string(),
			)
			.name(),
			schema,
		}
	}

	pub fn name_with_link(&self) -> WithLink {
		WithLink(&self.name)
	}

	pub fn description(&self) -> Option<String> {
		self.schema.get("description").and_then(|d| d.as_str()).map(Into::into)
	}

	pub fn inline<'root>(self, root_schema: &'root Value) -> TypeDoc<'root>
	where
		'doc: 'root,
	{
		match self.variant() {
			TypeVariant::Ref(type_ref) => type_ref.resolve(root_schema),
			_ => self,
		}
	}

	pub fn examples(&self) -> Option<&Vec<Value>> {
		self.schema.get("examples").and_then(|e| e.as_array())
	}

	pub fn example(&self) -> Option<Value> {
		if let Some(examples) = self.examples() {
			examples.first().cloned()
		} else {
			match self.variant() {
				TypeVariant::Enum(variants) => Some(serde_json::json!(variants
					.first()
					.expect("Enum must have at least one variant"))),
				TypeVariant::OneOf(options) => {
					let example =
						options.first().expect("OneOf must have at least one option").example()?;
					Some(serde_json::json!(example))
				},
				TypeVariant::Object(fields) => fields
					.iter()
					.map(|FieldDoc { name, type_doc, .. }| {
						Some((name.clone(), type_doc.example()?))
					})
					.collect::<Option<serde_json::Map<String, Value>>>()
					.map(Into::into),
				TypeVariant::Constant(c) => Some(serde_json::json!(c)),
				TypeVariant::Primitive(p) => p.example(),
				TypeVariant::Empty => Some(Value::Null),
				_ => None,
			}
		}
	}

	pub fn variant(&self) -> TypeVariant<'_> {
		// Special case for the empty/null type.
		if self.name == "Empty" {
			return TypeVariant::Empty;
		}

		let type_str = self.schema.get("type").and_then(|t| t.as_str());

		if let Some(options) =
			self.schema.get("oneOf").map(|o| o.as_array().expect("OneOf must be an array"))
		{
			return TypeVariant::OneOf(
				options
					.iter()
					.enumerate()
					.map(|(i, o)| {
						let suffix = if options.len() == 1 { "".into() } else { format!("_{}", i) };
						TypeDoc::new(&format!("{}{}", self.name, suffix), o)
					})
					.collect(),
			);
		}

		if let Some(properties) = self
			.schema
			.get("properties")
			.map(|p| p.as_object().expect("Properties must be an object"))
		{
			assert_eq!(type_str, Some("object"), "Only object types can have properties",);
			let requireds = self
				.schema
				.get("required")
				.map(|r| r.as_array().expect("Invalid `required` field, not an array"));

			return TypeVariant::Object(
				properties
					.iter()
					.map(|(name, field)| {
						FieldDoc::new(
							name,
							requireds.is_some_and(|requireds| {
								requireds.contains(&Value::String(name.clone()))
							}),
							field,
						)
					})
					.collect(),
			);
		}

		if let Some("array") = type_str {
			let items = self.schema.get("items").or_else(|| self.schema.get("prefixItems"));
			if let Some(item) = items.filter(|i| i.is_object()) {
				// Single item
				let mut inner = TypeDoc::new("temp", item);
				let name = match inner.variant() {
					TypeVariant::Ref(type_ref) => type_ref.name(),
					TypeVariant::Constant(c) => c,
					TypeVariant::Primitive(p) => p.to_string(),
					_ => panic!("Unexpected array item type: {}", inner.variant()),
				};
				inner.name = name;
				return TypeVariant::Array(vec![inner]);
			}
			if let Some(items) = items.and_then(|i| i.as_array()) {
				// Multiple items
				let inner = items
					.iter()
					.map(|item| {
						let mut inner = TypeDoc::new("temp", item);
						let name = match inner.variant() {
							TypeVariant::Ref(type_ref) => type_ref.name(),
							TypeVariant::Constant(c) => c,
							TypeVariant::Primitive(p) => p.to_string(),
							_ => panic!("Unexpected array item type: {}", inner.variant()),
						};
						inner.name = name;
						inner
					})
					.collect();
				return TypeVariant::Array(inner);
			}
		}

		if let Some(type_ref) =
			self.schema.get("$ref").map(|r| r.as_str().expect("Invalid type reference"))
		{
			return TypeVariant::Ref(TypeRef::new(type_ref.to_string()));
		}

		if let Some(variants) = self.schema.get("enum") {
			return TypeVariant::Enum(
				variants
					.as_array()
					.expect("Enum values must be an array")
					.iter()
					.map(|v| v.to_string())
					.collect(),
			);
		}

		if let Some(constant) = self.schema.get("const") {
			return TypeVariant::Constant(constant.to_string());
		}

		if let Some(primitive) = type_str {
			if primitive == "integer" {
				if let Some(format) = self.schema.get("format").and_then(|t| t.as_str()) {
					return TypeVariant::Primitive(PrimitiveType::Number(Some(
						NumberFormat::from_str(format).unwrap(),
					)));
				}
			}

			return TypeVariant::Primitive(match primitive {
				"integer" => PrimitiveType::Number(None),
				"boolean" => PrimitiveType::Boolean,
				"string" => PrimitiveType::String,
				"null" => PrimitiveType::Null,
				_ => panic!(
					"Unexpected primitive type: {} for schema {}: {}",
					primitive, self.name, self.schema
				),
			});
		}

		TypeVariant::RawSchema(self.schema)
	}
}

// TODO: parse on construction into a tree of types and fields
impl std::fmt::Display for TypeVariant<'_> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			TypeVariant::Empty => {
				if f.alternate() {
					writeln!(f, "This Request takes no parameters.")?;
				} else {
					writeln!(f, "`{{}}`")?;
				}
				Ok(())
			},
			TypeVariant::OneOf(options) => {
				if f.alternate() {
					let bullet = if options.len() > 1 && f.alternate() {
						writeln!(f, "One of the following alternatives:")?;
						writeln!(f)?;
						"- "
					} else {
						""
					};
					for option in options {
						writeln!(
							f,
							"{bullet}{}",
							option
								.description()
								.unwrap_or_else(|| format!("{}", option.name_with_link()))
						)?;
					}
				} else {
					// TODO: If all options are objects of the same shape, build a table.
					// if options.len() > 1 && options.iter().all(|o| matches!(o.variant(),
					// TypeVariant::Object(_)))
					write!(
						f,
						"{}",
						options
							.iter()
							.map(|o| format!("{}", o.variant()))
							.collect::<Vec<_>>()
							.join("**OR** ")
					)?;
				}
				Ok(())
			},
			TypeVariant::Object(fields) => {
				if f.alternate() {
					writeln!(f, "A JSON object with the following fields:")?;
					writeln!(f)?;
					for FieldDoc { name, type_doc, required } in fields {
						writeln!(
							f,
							"- {}{}: {}",
							name,
							if *required { "[Required]" } else { "[Optional]" },
							type_doc
								.description()
								.unwrap_or_else(|| format!("{}", type_doc.variant()))
						)?;
					}
				} else {
					writeln!(f, "Object {{")?;
					for FieldDoc { name, type_doc, required } in fields {
						writeln!(
							f,
							"  {name}{}: {}",
							if *required { "[Required]" } else { "[Optional]" },
							type_doc.variant()
						)?;
					}
					writeln!(f, "}}")?;
				}
				Ok(())
			},
			TypeVariant::Array(items) => {
				write!(
					f,
					"[{}]",
					items
						.iter()
						.map(|i| format!("{}", i.variant()))
						.collect::<Vec<_>>()
						.join(" | ")
				)
			},
			TypeVariant::Primitive(primitive) => {
				write!(f, "`{}`", primitive)
			},
			TypeVariant::Enum(variants) => {
				write!(f, "{}", variants.join(" | "))
			},
			TypeVariant::Ref(type_ref) => {
				write!(f, "{}", WithLink(&type_ref.name()))
			},
			TypeVariant::RawSchema(schema) => {
				write!(f, "`{}`", schema)
			},
			TypeVariant::Constant(constant) => {
				write!(f, "`{}`", constant)
			},
		}
	}
}

// Schema analysis
struct SchemaAnalyzer;

impl SchemaAnalyzer {
	fn analyze(schema: &Value) -> Result<ApiDocs<'_>, Box<dyn Error>> {
		let methods: Vec<_> = schema["methods"]
			.as_array()
			.expect("Methods must be an array")
			.iter()
			.map(|def| {
				let method = def["method"].as_str().expect("Method must be a string").to_string();
				MethodDef {
					method,
					request: TypeDoc::from_ref(&def["request"]).inline(schema),
					response: TypeDoc::from_ref(&def["response"]).inline(schema),
				}
			})
			.collect();

		let mut seen = methods
			.iter()
			.flat_map(|MethodDef { request, response, .. }| [&request.name[..], &response.name[..]])
			.collect::<BTreeSet<_>>();

		let request_defs = schema["$defs"]["request"]
			.as_object()
			.expect("Definitions must be an object")
			.iter();
		let response_defs = schema["$defs"]["response"]
			.as_object()
			.expect("Definitions must be an object")
			.iter();

		let types = request_defs
			.chain(response_defs)
			.filter_map(|(name, schema)| {
				if seen.contains(&name[..]) {
					None
				} else {
					seen.insert(name);
					Some(TypeDoc::new(name, schema))
				}
			})
			.collect::<Vec<_>>();

		Ok(ApiDocs { methods, types })
	}
}

/// Double the indentation of a line.
///
/// Used for adjusting the indentation of pretty-printed json, which uses 2 spaces,
/// to match the 4 spaces used by the rest of the markdown.
trait StringExt {
	fn double_indented(&self) -> String;
}

impl<S: AsRef<str>> StringExt for S {
	fn double_indented(&self) -> String {
		fn double_indent_line(line: &str) -> String {
			let indents = line.len() - line.trim_start().len();
			format!("{}{}", " ".repeat(indents), line)
		}
		self.as_ref().lines().map(double_indent_line).collect::<Vec<_>>().join("\n")
	}
}

// Markdown generation
struct MarkdownGenerator;

impl MarkdownGenerator {
	fn generate<W: io::Write>(writer: &mut W, docs: &ApiDocs) -> Result<(), Box<dyn Error>> {
		writeln!(writer, "# API Documentation")?;
		writeln!(writer)?;
		writeln!(writer, "## Introduction")?;
		writeln!(writer)?;
		writeln!(writer, "This API uses JSON-RPC 2.0 for all endpoints. To make a request, send a POST request with a JSON body containing:")?;
		writeln!(writer)?;
		writeln!(writer, "- `jsonrpc`: Must be \"2.0\"")?;
		writeln!(writer, "- `id`: A unique identifier for the request")?;
		writeln!(writer, "- `method`: The method name")?;
		writeln!(writer, "- `params`: The parameters for the method")?;
		writeln!(writer)?;
		writeln!(
			writer,
			"All requests should be sent as HTTP POST with Content-Type: application/json."
		)?;
		writeln!(writer)?;

		for MethodDef { method, request, response } in &docs.methods {
			writeln!(writer, "## {}\n", method)?;

			for (title, ty) in &[("Request", request), ("Response", response)] {
				writeln!(writer, "### {title}")?;
				writeln!(writer)?;

				// Note this uses the `#` alternate representation.
				writeln!(writer, "{:#}", ty.variant())?;
			}

			if let Some(example) = request.example() {
				writeln!(writer, "#### Example Request")?;

				let body = if example.is_null() {
					serde_json::json!({
						"jsonrpc": "2.0",
						"id": 1,
						"method": method,
					})
				} else {
					serde_json::json!({
						"jsonrpc": "2.0",
						"id": 1,
						"method": method,
						"params": example
					})
				};
				writeln!(
					writer,
					r#"
```bash copy
curl -X POST http://localhost:80 \
    -H 'Content-Type: application/json' \
    -d '{}'
```"#,
					serde_json::to_string_pretty(&body)?.double_indented()
				)?;
				writeln!(writer)?;
			}

			if let Some(example) = response.example() {
				writeln!(writer, "#### Example Response")?;
				writeln!(writer)?;
				writeln!(
					writer,
					"```json\n{}\n```",
					serde_json::to_string_pretty(&example)?.double_indented()
				)?;
				writeln!(writer)?;
			}
		}

		writeln!(writer, "## Types")?;
		writeln!(writer)?;

		for ty in docs.types.iter() {
			writeln!(writer, "### {}", ty.name)?;
			writeln!(writer)?;

			if let Some(description) = ty.description() {
				writeln!(writer, "{}", description)?;
				writeln!(writer)?;
			} else {
				writeln!(writer, "{}", ty.variant())?;
				writeln!(writer)?;
			}
			if let Some(examples) = ty.examples() {
				writeln!(writer, "Examples:")?;
				writeln!(writer)?;
				writeln!(writer, "```json")?;
				for example in examples {
					writeln!(writer, "{}", serde_json::to_string_pretty(&example)?)?;
				}
				writeln!(writer, "```")?;
				writeln!(writer)?;
			}
		}
		Ok(())
	}
}

#[derive(clap::Parser)]
#[command(author, version, about = "Generate API documentation from JSON Schema")]
struct Args {
	/// Input JSON Schema file
	#[arg(short, long)]
	schema: String,

	/// Output markdown file (defaults to stdout if not provided)
	#[arg(short, long)]
	output: Option<String>,

	/// Debug mode prints the input schema to stdout before analysis.
	#[arg(short, long)]
	debug: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
	let args = Args::parse();

	// Read and parse schema
	let schema_file = fs::File::open(&args.schema)?;

	// Phase 1: Analyze schema into documentation model
	let schema = serde_json::from_reader(schema_file)?;
	let docs = SchemaAnalyzer::analyze(&schema)?;

	if args.debug {
		println!("{:#?}", docs.methods);
	}

	// Phase 2: Generate markdown
	match args.output {
		Some(path) => {
			MarkdownGenerator::generate(&mut fs::File::create(path)?, &docs)?;
		},
		None => {
			MarkdownGenerator::generate(&mut io::stdout().lock(), &docs)?;
		},
	}

	Ok(())
}
