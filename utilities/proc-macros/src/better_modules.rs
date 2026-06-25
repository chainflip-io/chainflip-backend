use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
	braced, parenthesized,
	parse::{Parse, ParseStream},
	token,
	visit_mut::{self, VisitMut},
	GenericArgument, Ident, Item, ItemImpl, ItemMacro, ItemMod, ItemStruct, ItemType,
	PathArguments, Token, TypeParam,
};

// ─── Input parsing ────────────────────────────────────────────────────────────

/// Top-level input: `mod (A: Trait) (B: Trait) { ... }`
pub struct Input {
	pub telescope: Vec<TypeParam>,
	pub items: Vec<ModuleItem>,
}

/// Items that can appear inside a `better_modules!` block.
pub enum ModuleItem {
	TypeAlias(ItemType),
	Struct(ItemStruct),
	Impl(ItemImpl),
	Mod(ItemMod),
	MacroCall(ItemMacro),
	Conditional(Conditional),
	Other(Item),
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

		Ok(Input { telescope, items })
	}
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

	// Otherwise parse as a standard Rust item and classify
	let item: Item = input.parse()?;
	Ok(match item {
		Item::Type(t) => ModuleItem::TypeAlias(t),
		Item::Struct(s) => ModuleItem::Struct(s),
		Item::Impl(i) => ModuleItem::Impl(i),
		Item::Mod(m) => ModuleItem::Mod(m),
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

/// Tracks which items have been defined within the macro and what telescope
/// params were added to them. Used to rewrite references automatically.
#[derive(Clone, Default)]
struct Definitions {
	/// Maps item name → telescope param idents that were added to its definition.
	map: HashMap<String, Vec<Ident>>,
}

impl Definitions {
	fn register(&mut self, name: &Ident, tele_params: &[&TypeParam]) {
		let idents: Vec<Ident> = tele_params.iter().map(|p| p.ident.clone()).collect();
		if !idents.is_empty() {
			self.map.insert(name.to_string(), idents);
		}
	}
}

/// Visitor that rewrites type paths: if a path segment matches a known
/// definition, append its telescope params as generic arguments.
struct RewriteVisitor<'a> {
	defs: &'a Definitions,
}

impl VisitMut for RewriteVisitor<'_> {
	fn visit_type_path_mut(&mut self, type_path: &mut syn::TypePath) {
		// Visit nested types first
		visit_mut::visit_type_path_mut(self, type_path);

		// Check the last segment of the path (e.g. `Foo` in `self::Foo` or just `Foo`)
		if let Some(last_seg) = type_path.path.segments.last_mut() {
			if let Some(tele_idents) = self.defs.map.get(&last_seg.ident.to_string()) {
				// Append telescope idents as type arguments
				let args = match &mut last_seg.arguments {
					PathArguments::None => {
						let args: syn::AngleBracketedGenericArguments = syn::parse_quote! {
							< #(#tele_idents),* >
						};
						last_seg.arguments = PathArguments::AngleBracketed(args);
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
}

fn rewrite_item_type(item: &mut ItemType, defs: &Definitions) {
	let mut visitor = RewriteVisitor { defs };
	visitor.visit_type_mut(&mut item.ty);
}

fn rewrite_item_struct(item: &mut ItemStruct, defs: &Definitions) {
	let mut visitor = RewriteVisitor { defs };
	visitor.visit_item_struct_mut(item);
}

fn rewrite_item_impl(item: &mut ItemImpl, defs: &Definitions) {
	let mut visitor = RewriteVisitor { defs };
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
	expand_items(&input.telescope, &input.items, &mut defs)
}

fn expand_items(
	telescope: &[TypeParam],
	items: &[ModuleItem],
	defs: &mut Definitions,
) -> TokenStream {
	let mut output = TokenStream::new();
	for item in items {
		output.extend(expand_item(telescope, item, defs));
	}
	output
}

fn expand_item(telescope: &[TypeParam], item: &ModuleItem, defs: &mut Definitions) -> TokenStream {
	match item {
		ModuleItem::TypeAlias(t) => expand_type_alias(telescope, t, defs),
		ModuleItem::Struct(s) => expand_struct(telescope, s, defs),
		ModuleItem::Impl(i) => expand_impl(telescope, i, defs),
		ModuleItem::Mod(m) => expand_mod(telescope, m, defs),
		ModuleItem::MacroCall(m) => expand_macro_call(telescope, m, defs),
		ModuleItem::Conditional(c) => expand_conditional(telescope, c, defs),
		ModuleItem::Other(item) => quote! { #item },
	}
}

fn expand_type_alias(
	telescope: &[TypeParam],
	item: &ItemType,
	defs: &mut Definitions,
) -> TokenStream {
	let mut item = item.clone();

	// First rewrite references to previously defined items
	rewrite_item_type(&mut item, defs);

	// Only add telescope params that appear in the (rewritten) type definition
	let ty_tokens = quote! { #item.ty };
	let used = used_telescope_params(telescope, &ty_tokens);

	// Register this definition before adding params to generics
	defs.register(&item.ident, &used);

	for param in &used {
		item.generics.params.push(syn::GenericParam::Type((*param).clone()));
	}

	quote! { #item }
}

fn expand_struct(
	telescope: &[TypeParam],
	item: &ItemStruct,
	defs: &mut Definitions,
) -> TokenStream {
	let mut item = item.clone();

	// First rewrite references to previously defined items in field types
	rewrite_item_struct(&mut item, defs);

	// Register this definition with ALL telescope params
	let all_params: Vec<&TypeParam> = telescope.iter().collect();
	defs.register(&item.ident, &all_params);

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

fn expand_impl(telescope: &[TypeParam], item: &ItemImpl, defs: &mut Definitions) -> TokenStream {
	let mut item = item.clone();

	// Rewrite references (self type, trait path, body)
	rewrite_item_impl(&mut item, defs);

	// Determine which telescope params are used in the (rewritten) impl
	let impl_tokens = quote! { #item };
	let used = used_telescope_params(telescope, &impl_tokens);

	for param in used {
		item.generics.params.push(syn::GenericParam::Type(param.clone()));
	}

	quote! { #item }
}

fn expand_mod(telescope: &[TypeParam], item: &ItemMod, defs: &mut Definitions) -> TokenStream {
	let vis = &item.vis;
	let ident = &item.ident;

	// Separate outer attributes (before `mod`) from inner attributes (inside braces)
	let outer_attrs: Vec<_> =
		item.attrs.iter().filter(|a| matches!(a.style, syn::AttrStyle::Outer)).collect();
	let inner_attrs: Vec<_> =
		item.attrs.iter().filter(|a| matches!(a.style, syn::AttrStyle::Inner(_))).collect();

	match &item.content {
		Some((_brace, items)) => {
			// Nested modules get their own definitions scope (clone parent defs)
			let mut inner_defs = defs.clone();
			let inner: Vec<ModuleItem> = items.iter().map(|i| classify_item(i.clone())).collect();
			let expanded = expand_items(telescope, &inner, &mut inner_defs);

			// Propagate definitions from the inner module back to the parent,
			// so that path-qualified references like `module::Item` can be resolved.
			for (name, tele_idents) in &inner_defs.map {
				if !defs.map.contains_key(name) {
					defs.map.insert(name.clone(), tele_idents.clone());
				}
			}

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

fn expand_conditional(
	telescope: &[TypeParam],
	cond: &Conditional,
	defs: &mut Definitions,
) -> TokenStream {
	let branch = match &cond.condition {
		Some(_) => &cond.true_branch,
		None => &cond.false_branch,
	};
	expand_items(telescope, branch, defs)
}

fn expand_macro_call(
	telescope: &[TypeParam],
	item: &ItemMacro,
	defs: &mut Definitions,
) -> TokenStream {
	let mut item = item.clone();

	// Try to parse the macro body as a sequence of items and process them.
	// If parsing succeeds, re-emit the macro with the processed body.
	// If it fails, emit the macro verbatim.
	let body_tokens = item.mac.tokens.clone();
	let parsed: syn::Result<syn::File> = syn::parse2(quote! { #body_tokens });

	if let Ok(file) = parsed {
		let module_items: Vec<ModuleItem> =
			file.items.into_iter().map(classify_item).collect();
		let expanded_body = expand_items(telescope, &module_items, defs);
		item.mac.tokens = expanded_body;
	}

	quote! { #item }
}

fn classify_item(item: Item) -> ModuleItem {
	match item {
		Item::Type(t) => ModuleItem::TypeAlias(t),
		Item::Struct(s) => ModuleItem::Struct(s),
		Item::Impl(i) => ModuleItem::Impl(i),
		Item::Mod(m) => ModuleItem::Mod(m),
		Item::Macro(m) => ModuleItem::MacroCall(m),
		other => ModuleItem::Other(other),
	}
}
