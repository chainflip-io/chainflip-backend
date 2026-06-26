use std::collections::{HashMap, HashSet};

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
	braced, parenthesized,
	parse::{Parse, ParseStream},
	token,
	visit_mut::{self, VisitMut},
	Attribute, ExprPath, GenericArgument, Generics, Ident, Item, ItemImpl, ItemMacro, ItemMod,
	ItemStruct, ItemTrait, ItemType, ItemUse, PathArguments, Token, TraitBound, TypeParam, UseTree,
	Visibility,
};

// ─── Input parsing ────────────────────────────────────────────────────────────

/// Top-level input: a sequence of Rust items plus optional telescope scopes like
/// `mod (A: Trait) (B: Trait) { ... }`.
pub struct Input {
	pub items: Vec<ModuleItem>,
}

/// Items that can appear inside a `better_modules!` block.
pub enum ModuleItem {
	TypeAlias(ItemType),
	Struct(ItemStruct),
	Trait(ItemTrait),
	Impl(ItemImpl),
	Mod(ItemMod),
	PlainMod(PlainMod),
	TelescopeMod(TelescopeMod),
	Use(ItemUse),
	MacroCall(ItemMacro),
	Conditional(Conditional),
	Other(Item),
}

/// `mod foo { ... }` or `mod foo;` parsed with `better_modules` items in the body.
pub struct PlainMod {
	pub attrs: Vec<Attribute>,
	pub vis: Visibility,
	pub ident: Ident,
	pub items: Option<Vec<ModuleItem>>,
}

/// `mod (A: Trait) (B: Trait) { ... }`
pub struct TelescopeMod {
	pub telescope: Vec<TypeParam>,
	pub items: Vec<ModuleItem>,
}

/// `if (condition) { ... } else { ... }` or `if () { ... } else { ... }`
pub struct Conditional {
	pub condition: Option<TokenStream>,
	pub true_branch: Vec<ModuleItem>,
	pub false_branch: Vec<ModuleItem>,
}

// ─── Parsing implementations ──────────────────────────────────────────────────

impl Parse for Input {
	fn parse(input: ParseStream) -> syn::Result<Self> {
		let items = parse_module_items(input)?;

		Ok(Input { items })
	}
}

fn parse_telescope_mod(input: ParseStream) -> syn::Result<TelescopeMod> {
	input.parse::<Token![mod]>()?;

	// Parse telescope: (A: Trait) (B: Trait) ...
	let mut telescope = Vec::new();
	while input.peek(token::Paren) {
		let content;
		parenthesized!(content in input);
		telescope.push(content.parse::<TypeParam>()?);
	}

	// Parse braced body
	let body;
	braced!(body in input);
	let items = parse_module_items(&body)?;

	Ok(TelescopeMod { telescope, items })
}

fn parse_plain_mod(input: ParseStream) -> syn::Result<PlainMod> {
	let attrs = input.call(Attribute::parse_outer)?;
	let vis = input.parse::<Visibility>()?;
	input.parse::<Token![mod]>()?;
	let ident = input.parse::<Ident>()?;

	let items = if input.peek(Token![;]) {
		input.parse::<Token![;]>()?;
		None
	} else {
		let body;
		braced!(body in input);
		Some(parse_module_items(&body)?)
	};

	Ok(PlainMod { attrs, vis, ident, items })
}

fn parse_module_items(input: ParseStream) -> syn::Result<Vec<ModuleItem>> {
	let mut items = Vec::new();
	while !input.is_empty() {
		items.push(parse_module_item(input)?);
	}
	Ok(items)
}

fn parse_module_item(input: ParseStream) -> syn::Result<ModuleItem> {
	// Check for `if` conditional
	if input.peek(Token![if]) {
		return Ok(ModuleItem::Conditional(parse_conditional(input)?));
	}

	{
		let fork = input.fork();
		let attrs = fork.call(Attribute::parse_outer)?;
		let vis = fork.parse::<Visibility>()?;

		if fork.peek(Token![mod]) {
			fork.parse::<Token![mod]>()?;
			if fork.peek(token::Paren) {
				if attrs.is_empty() && matches!(vis, Visibility::Inherited) {
					return Ok(ModuleItem::TelescopeMod(parse_telescope_mod(input)?));
				}
			} else {
				return Ok(ModuleItem::PlainMod(parse_plain_mod(input)?));
			}
		}
	}

	// Otherwise parse as a standard Rust item and classify
	let item: Item = input.parse()?;
	Ok(match item {
		Item::Type(t) => ModuleItem::TypeAlias(t),
		Item::Struct(s) => ModuleItem::Struct(s),
		Item::Trait(t) => ModuleItem::Trait(t),
		Item::Impl(i) => ModuleItem::Impl(i),
		Item::Mod(m) => ModuleItem::Mod(m),
		Item::Use(u) => ModuleItem::Use(u),
		Item::Macro(m) => ModuleItem::MacroCall(m),
		other => ModuleItem::Other(other),
	})
}

fn parse_conditional(input: ParseStream) -> syn::Result<Conditional> {
	input.parse::<Token![if]>()?;

	// Parse condition in parentheses
	let cond_content;
	parenthesized!(cond_content in input);
	let condition: TokenStream = cond_content.parse()?;
	let condition = if condition.is_empty() { None } else { Some(condition) };

	// Parse true branch
	let true_body;
	braced!(true_body in input);
	let true_branch = parse_module_items(&true_body)?;

	// Parse else branch
	input.parse::<Token![else]>()?;
	let false_body;
	braced!(false_body in input);
	let false_branch = parse_module_items(&false_body)?;

	Ok(Conditional { condition, true_branch, false_branch })
}

// ─── Code generation ──────────────────────────────────────────────────────────

type ScopePath = Vec<String>;

#[derive(Clone)]
enum ImportTarget {
	Definition(Vec<Ident>),
	Module(ScopePath),
}

/// Tracks local items produced by the macro. Definitions are keyed by module
/// path so qualified paths can be resolved before telescope params are added.
#[derive(Clone, Default)]
struct Definitions {
	definitions: HashMap<ScopePath, Vec<Ident>>,
	modules: HashSet<ScopePath>,
	imports: HashMap<ScopePath, HashMap<String, ImportTarget>>,
}

impl Definitions {
	fn register(&mut self, scope: &[String], name: &Ident, tele_params: &[&TypeParam]) {
		let idents: Vec<Ident> = tele_params.iter().map(|p| p.ident.clone()).collect();
		let mut path = scope.to_vec();
		path.push(name.to_string());
		self.definitions.insert(path, idents);
	}

	fn register_module(&mut self, scope: &[String], name: &Ident) -> ScopePath {
		let mut path = scope.to_vec();
		path.push(name.to_string());
		self.modules.insert(path.clone());
		path
	}

	fn register_use(&mut self, scope: &[String], item: &ItemUse) {
		let mut prefix = Vec::new();
		self.register_use_tree(scope, &mut prefix, &item.tree);
	}

	fn register_use_tree(&mut self, scope: &[String], prefix: &mut ScopePath, tree: &UseTree) {
		match tree {
			UseTree::Path(path) => {
				prefix.push(path.ident.to_string());
				self.register_use_tree(scope, prefix, &path.tree);
				prefix.pop();
			},
			UseTree::Name(name) => {
				let mut target = prefix.clone();
				target.push(name.ident.to_string());
				self.register_import(scope, name.ident.to_string(), &target);
			},
			UseTree::Rename(rename) => {
				let mut target = prefix.clone();
				target.push(rename.ident.to_string());
				self.register_import(scope, rename.rename.to_string(), &target);
			},
			UseTree::Glob(_) =>
				if let Some(module_path) = self.resolve_module_path(scope, prefix) {
					for (path, params) in self.definitions.clone() {
						if path.len() == module_path.len() + 1 && path.starts_with(&module_path) {
							if let Some(name) = path.last() {
								self.imports
									.entry(scope.to_vec())
									.or_default()
									.insert(name.clone(), ImportTarget::Definition(params));
							}
						}
					}
					for path in self.modules.clone() {
						if path.len() == module_path.len() + 1 && path.starts_with(&module_path) {
							if let Some(name) = path.last() {
								self.imports
									.entry(scope.to_vec())
									.or_default()
									.insert(name.clone(), ImportTarget::Module(path));
							}
						}
					}
				},
			UseTree::Group(group) =>
				for tree in &group.items {
					self.register_use_tree(scope, prefix, tree);
				},
		}
	}

	fn register_import(&mut self, scope: &[String], alias: String, target: &[String]) {
		if let Some(params) = self.resolve_definition_path(scope, target) {
			self.imports
				.entry(scope.to_vec())
				.or_default()
				.insert(alias, ImportTarget::Definition(params));
		} else if let Some(module_path) = self.resolve_module_path(scope, target) {
			self.imports
				.entry(scope.to_vec())
				.or_default()
				.insert(alias, ImportTarget::Module(module_path));
		}
	}

	fn resolve_path_prefix(
		&self,
		scope: &[String],
		path: &syn::Path,
		generic_idents: &[Ident],
	) -> Option<(usize, Vec<Ident>)> {
		if path.leading_colon.is_some() {
			return None;
		}

		let segments: Vec<String> = path.segments.iter().map(|seg| seg.ident.to_string()).collect();
		if segments.is_empty() {
			return None;
		}

		if segments.len() > 1 && generic_idents.iter().any(|ident| ident == &path.segments[0].ident)
		{
			return None;
		}

		for len in (1..=segments.len()).rev() {
			if let Some(params) = self.resolve_definition_path(scope, &segments[..len]) {
				return Some((len - 1, params));
			}
		}

		None
	}

	fn resolve_definition_path(&self, scope: &[String], segments: &[String]) -> Option<Vec<Ident>> {
		if segments.is_empty() {
			return None;
		}

		if segments.len() == 1 {
			if let Some(params) =
				self.definitions.get(&scope.iter().chain(segments).cloned().collect::<Vec<_>>())
			{
				return Some(params.clone());
			}

			if let Some(ImportTarget::Definition(params)) =
				self.imports.get(scope).and_then(|imports| imports.get(&segments[0]))
			{
				return Some(params.clone());
			}
			return None;
		}

		if let Some(resolved) = self.resolve_path_starting_with_import(scope, segments) {
			return self.definitions.get(&resolved).cloned();
		}

		let candidate = scope.iter().chain(segments).cloned().collect::<Vec<_>>();
		self.definitions.get(&candidate).cloned()
	}

	fn resolve_module_path(&self, scope: &[String], segments: &[String]) -> Option<ScopePath> {
		if segments.is_empty() {
			return Some(scope.to_vec());
		}

		if let Some(resolved) = self.resolve_path_starting_with_import(scope, segments) {
			if self.modules.contains(&resolved) {
				return Some(resolved);
			}
		}

		let candidate = scope.iter().chain(segments).cloned().collect::<Vec<_>>();
		if self.modules.contains(&candidate) {
			return Some(candidate);
		}

		None
	}

	fn resolve_path_starting_with_import(
		&self,
		scope: &[String],
		segments: &[String],
	) -> Option<ScopePath> {
		match segments.first().map(String::as_str) {
			Some("self") => Some(scope.iter().chain(segments.iter().skip(1)).cloned().collect()),
			Some("super") => {
				let mut resolved = scope.to_vec();
				let mut rest = segments;
				while matches!(rest.first().map(String::as_str), Some("super")) {
					resolved.pop();
					rest = &rest[1..];
				}
				resolved.extend(rest.iter().cloned());
				Some(resolved)
			},
			Some("crate") => None,
			Some(first) => match self.imports.get(scope).and_then(|imports| imports.get(first)) {
				Some(ImportTarget::Module(module_path)) =>
					Some(module_path.iter().chain(segments.iter().skip(1)).cloned().collect()),
				_ => None,
			},
			None => None,
		}
	}
}

/// Visitor that rewrites type paths: if a path segment matches a known
/// definition, append its telescope params as generic arguments.
struct RewriteVisitor<'a> {
	defs: &'a Definitions,
	scope: &'a [String],
	generic_idents: Vec<Ident>,
}

impl VisitMut for RewriteVisitor<'_> {
	fn visit_trait_bound_mut(&mut self, trait_bound: &mut TraitBound) {
		visit_mut::visit_trait_bound_mut(self, trait_bound);
		self.rewrite_path(&mut trait_bound.path);
	}

	fn visit_item_impl_mut(&mut self, item_impl: &mut ItemImpl) {
		visit_mut::visit_item_impl_mut(self, item_impl);

		if let Some((_bang, trait_path, _for_token)) = &mut item_impl.trait_ {
			self.rewrite_path(trait_path);
		}
	}

	fn visit_type_path_mut(&mut self, type_path: &mut syn::TypePath) {
		// Visit nested types first
		visit_mut::visit_type_path_mut(self, type_path);

		if type_path.qself.is_some() {
			return;
		}

		self.rewrite_path(&mut type_path.path);
	}

	fn visit_expr_path_mut(&mut self, expr_path: &mut ExprPath) {
		visit_mut::visit_expr_path_mut(self, expr_path);

		if expr_path.qself.is_some() {
			return;
		}

		self.rewrite_path(&mut expr_path.path);
	}
}

impl RewriteVisitor<'_> {
	fn rewrite_path(&self, path: &mut syn::Path) {
		let Some((segment_index, tele_idents)) =
			self.defs.resolve_path_prefix(self.scope, path, &self.generic_idents)
		else {
			return;
		};

		if tele_idents.is_empty() {
			return;
		}

		if let Some(segment) = path.segments.iter_mut().nth(segment_index) {
			let args = match &mut segment.arguments {
				PathArguments::None => {
					let args: syn::AngleBracketedGenericArguments = syn::parse_quote! {
						< #(#tele_idents),* >
					};
					segment.arguments = PathArguments::AngleBracketed(args);
					return;
				},
				PathArguments::AngleBracketed(existing) => existing,
				PathArguments::Parenthesized(_) => return,
			};
			for ident in tele_idents {
				let arg: GenericArgument = syn::parse_quote! { #ident };
				args.args.push(arg);
			}
		}
	}
}

fn generic_idents(telescope: &[TypeParam], generics: &Generics) -> Vec<Ident> {
	telescope
		.iter()
		.map(|param| param.ident.clone())
		.chain(generics.type_params().map(|param| param.ident.clone()))
		.collect()
}

fn rewrite_item_type_with_telescope(
	item: &mut ItemType,
	defs: &Definitions,
	telescope: &[TypeParam],
	scope: &[String],
) {
	let mut visitor =
		RewriteVisitor { defs, scope, generic_idents: generic_idents(telescope, &item.generics) };
	visitor.visit_type_mut(&mut item.ty);
}

fn rewrite_item_struct(
	item: &mut ItemStruct,
	defs: &Definitions,
	telescope: &[TypeParam],
	scope: &[String],
) {
	let mut visitor =
		RewriteVisitor { defs, scope, generic_idents: generic_idents(telescope, &item.generics) };
	visitor.visit_item_struct_mut(item);
}

fn rewrite_item_trait(
	item: &mut ItemTrait,
	defs: &Definitions,
	telescope: &[TypeParam],
	scope: &[String],
) {
	let mut visitor =
		RewriteVisitor { defs, scope, generic_idents: generic_idents(telescope, &item.generics) };
	visitor.visit_item_trait_mut(item);
}

fn rewrite_item_impl(
	item: &mut ItemImpl,
	defs: &Definitions,
	telescope: &[TypeParam],
	scope: &[String],
) {
	let mut visitor =
		RewriteVisitor { defs, scope, generic_idents: generic_idents(telescope, &item.generics) };
	visitor.visit_item_impl_mut(item);
}

/// Returns true if `ident` appears as a whole word anywhere in `tokens`.
fn tokens_contain_ident(tokens: &TokenStream, ident: &Ident) -> bool {
	let s = tokens.to_string();
	let id = ident.to_string();
	s.split(|c: char| !c.is_alphanumeric() && c != '_').any(|word| word == id)
}

/// Filter telescope params to only those whose ident appears in `tokens`.
fn used_telescope_params<'a>(
	telescope: &'a [TypeParam],
	tokens: &TokenStream,
) -> Vec<&'a TypeParam> {
	telescope.iter().filter(|p| tokens_contain_ident(tokens, &p.ident)).collect()
}

pub fn expand(input: Input) -> TokenStream {
	let mut defs = Definitions::default();
	defs.modules.insert(Vec::new());
	expand_items(&[], &input.items, &mut defs, &[])
}

fn expand_items(
	telescope: &[TypeParam],
	items: &[ModuleItem],
	defs: &mut Definitions,
	scope: &[String],
) -> TokenStream {
	let mut output = TokenStream::new();
	for item in items {
		output.extend(expand_item(telescope, item, defs, scope));
	}
	output
}

fn expand_item(
	telescope: &[TypeParam],
	item: &ModuleItem,
	defs: &mut Definitions,
	scope: &[String],
) -> TokenStream {
	match item {
		ModuleItem::TypeAlias(t) => expand_type_alias(telescope, t, defs, scope),
		ModuleItem::Struct(s) => expand_struct(telescope, s, defs, scope),
		ModuleItem::Trait(t) => expand_trait(telescope, t, defs, scope),
		ModuleItem::Impl(i) => expand_impl(telescope, i, defs, scope),
		ModuleItem::Mod(m) => expand_mod(telescope, m, defs, scope),
		ModuleItem::PlainMod(m) => expand_plain_mod(telescope, m, defs, scope),
		ModuleItem::TelescopeMod(m) => expand_telescope_mod(telescope, m, defs, scope),
		ModuleItem::Use(u) => expand_use(u, defs, scope),
		ModuleItem::MacroCall(m) => expand_macro_call(telescope, m, defs, scope),
		ModuleItem::Conditional(c) => expand_conditional(telescope, c, defs, scope),
		ModuleItem::Other(item) => quote! { #item },
	}
}

fn expand_plain_mod(
	telescope: &[TypeParam],
	item: &PlainMod,
	defs: &mut Definitions,
	scope: &[String],
) -> TokenStream {
	let attrs = &item.attrs;
	let vis = &item.vis;
	let ident = &item.ident;
	match &item.items {
		Some(items) => {
			let inner_scope = defs.register_module(scope, ident);
			let expanded = expand_items(telescope, items, defs, &inner_scope);

			quote! {
				#(#attrs)*
				#vis mod #ident {
					#expanded
				}
			}
		},
		None => quote! { #(#attrs)* #vis mod #ident; },
	}
}

fn expand_telescope_mod(
	telescope: &[TypeParam],
	item: &TelescopeMod,
	defs: &mut Definitions,
	scope: &[String],
) -> TokenStream {
	let mut combined_telescope = telescope.to_vec();
	combined_telescope.extend(item.telescope.iter().cloned());
	expand_items(&combined_telescope, &item.items, defs, scope)
}

fn expand_type_alias(
	telescope: &[TypeParam],
	item: &ItemType,
	defs: &mut Definitions,
	scope: &[String],
) -> TokenStream {
	let mut item = item.clone();

	// First rewrite references to previously defined items
	rewrite_item_type_with_telescope(&mut item, defs, telescope, scope);

	// Only add telescope params that appear in the (rewritten) type definition
	let ty_tokens = quote! { #item.ty };
	let used = used_telescope_params(telescope, &ty_tokens);

	// Register this definition before adding params to generics
	defs.register(scope, &item.ident, &used);

	for param in &used {
		item.generics.params.push(syn::GenericParam::Type((*param).clone()));
	}

	quote! { #item }
}

fn expand_struct(
	telescope: &[TypeParam],
	item: &ItemStruct,
	defs: &mut Definitions,
	scope: &[String],
) -> TokenStream {
	let mut item = item.clone();

	// First rewrite references to previously defined items in field types
	rewrite_item_struct(&mut item, defs, telescope, scope);

	// Register this definition with ALL telescope params
	let all_params: Vec<&TypeParam> = telescope.iter().collect();
	defs.register(scope, &item.ident, &all_params);

	// Determine which telescope params are used in the struct fields
	let fields_tokens = match &item.fields {
		syn::Fields::Named(f) => quote! { #f },
		syn::Fields::Unnamed(f) => quote! { #f },
		syn::Fields::Unit => TokenStream::new(),
	};
	let fields_str = fields_tokens.to_string();

	// Add ALL telescope type params to generics
	for param in telescope {
		item.generics.params.push(syn::GenericParam::Type(param.clone()));
	}

	// Add PhantomData for telescope types that are NOT used in fields
	let unused_idents: Vec<&Ident> = telescope
		.iter()
		.map(|p| &p.ident)
		.filter(|id| {
			!fields_str
				.split(|c: char| !c.is_alphanumeric() && c != '_')
				.any(|w| w == id.to_string())
		})
		.collect();

	if !unused_idents.is_empty() {
		if let syn::Fields::Named(ref mut fields) = item.fields {
			let phantom_field: syn::Field = syn::parse_quote! {
				_phantom: core::marker::PhantomData<( #(#unused_idents,)* )>
			};
			fields.named.push(phantom_field);
		}
	}

	quote! { #item }
}

fn expand_trait(
	telescope: &[TypeParam],
	item: &ItemTrait,
	defs: &mut Definitions,
	scope: &[String],
) -> TokenStream {
	let mut item = item.clone();

	// First rewrite references to previously defined items in bounds and trait items.
	rewrite_item_trait(&mut item, defs, telescope, scope);

	// Only add telescope params that appear in the (rewritten) trait definition.
	let trait_tokens = quote! { #item };
	let used = used_telescope_params(telescope, &trait_tokens);

	// Register this definition before adding params to generics.
	defs.register(scope, &item.ident, &used);

	for param in &used {
		item.generics.params.push(syn::GenericParam::Type((*param).clone()));
	}

	quote! { #item }
}

fn expand_impl(
	telescope: &[TypeParam],
	item: &ItemImpl,
	defs: &mut Definitions,
	scope: &[String],
) -> TokenStream {
	let mut item = item.clone();

	// Rewrite references (self type, trait path, body)
	rewrite_item_impl(&mut item, defs, telescope, scope);

	// Determine which telescope params are used in the (rewritten) impl
	let impl_tokens = quote! { #item };
	let used = used_telescope_params(telescope, &impl_tokens);

	for param in used {
		item.generics.params.push(syn::GenericParam::Type(param.clone()));
	}

	quote! { #item }
}

fn expand_mod(
	telescope: &[TypeParam],
	item: &ItemMod,
	defs: &mut Definitions,
	scope: &[String],
) -> TokenStream {
	let vis = &item.vis;
	let ident = &item.ident;

	// Separate outer attributes (before `mod`) from inner attributes (inside braces)
	let outer_attrs: Vec<_> =
		item.attrs.iter().filter(|a| matches!(a.style, syn::AttrStyle::Outer)).collect();
	let inner_attrs: Vec<_> = item
		.attrs
		.iter()
		.filter(|a| matches!(a.style, syn::AttrStyle::Inner(_)))
		.collect();

	match &item.content {
		Some((_brace, items)) => {
			let inner_scope = defs.register_module(scope, ident);
			let inner: Vec<ModuleItem> = items.iter().map(|i| classify_item(i.clone())).collect();
			let expanded = expand_items(telescope, &inner, defs, &inner_scope);

			quote! {
				#(#outer_attrs)*
				#vis mod #ident {
					#(#inner_attrs)*
					#expanded
				}
			}
		},
		None => quote! { #(#outer_attrs)* #vis mod #ident; },
	}
}

fn expand_use(item: &ItemUse, defs: &mut Definitions, scope: &[String]) -> TokenStream {
	defs.register_use(scope, item);
	quote! { #item }
}

fn expand_conditional(
	telescope: &[TypeParam],
	cond: &Conditional,
	defs: &mut Definitions,
	scope: &[String],
) -> TokenStream {
	let branch = match &cond.condition {
		Some(_) => &cond.true_branch,
		None => &cond.false_branch,
	};
	expand_items(telescope, branch, defs, scope)
}

fn expand_macro_call(
	telescope: &[TypeParam],
	item: &ItemMacro,
	defs: &mut Definitions,
	scope: &[String],
) -> TokenStream {
	let mut item = item.clone();

	// Try to parse the macro body as a sequence of items and process them.
	// If parsing succeeds, re-emit the macro with the processed body.
	// If it fails, emit the macro verbatim.
	let body_tokens = item.mac.tokens.clone();
	let parsed: syn::Result<syn::File> = syn::parse2(quote! { #body_tokens });

	if let Ok(file) = parsed {
		let module_items: Vec<ModuleItem> = file.items.into_iter().map(classify_item).collect();
		let expanded_body = expand_items(telescope, &module_items, defs, scope);
		item.mac.tokens = expanded_body;
	}

	quote! { #item }
}

fn classify_item(item: Item) -> ModuleItem {
	match item {
		Item::Type(t) => ModuleItem::TypeAlias(t),
		Item::Struct(s) => ModuleItem::Struct(s),
		Item::Trait(t) => ModuleItem::Trait(t),
		Item::Impl(i) => ModuleItem::Impl(i),
		Item::Mod(m) => ModuleItem::Mod(m),
		Item::Use(u) => ModuleItem::Use(u),
		Item::Macro(m) => ModuleItem::MacroCall(m),
		other => ModuleItem::Other(other),
	}
}
