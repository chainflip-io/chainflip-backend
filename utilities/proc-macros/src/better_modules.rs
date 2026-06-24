use proc_macro2::TokenStream;
use quote::quote;
use syn::{
	braced, parenthesized,
	parse::{Parse, ParseStream},
	token, Ident, Item, ItemImpl, ItemMod, ItemStruct, ItemType, Token, TypeParam,
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

pub fn expand(input: Input) -> TokenStream {
	expand_items(&input.telescope, &input.items)
}

fn expand_items(telescope: &[TypeParam], items: &[ModuleItem]) -> TokenStream {
	let expanded: Vec<TokenStream> =
		items.iter().map(|item| expand_item(telescope, item)).collect();
	quote! { #(#expanded)* }
}

fn expand_item(telescope: &[TypeParam], item: &ModuleItem) -> TokenStream {
	match item {
		ModuleItem::TypeAlias(t) => expand_type_alias(telescope, t),
		ModuleItem::Struct(s) => expand_struct(telescope, s),
		ModuleItem::Impl(i) => expand_impl(telescope, i),
		ModuleItem::Mod(m) => expand_mod(telescope, m),
		ModuleItem::Conditional(c) => expand_conditional(telescope, c),
		ModuleItem::Other(item) => quote! { #item },
	}
}

fn expand_type_alias(telescope: &[TypeParam], item: &ItemType) -> TokenStream {
	let mut item = item.clone();
	for param in telescope {
		item.generics.params.push(syn::GenericParam::Type(param.clone()));
	}
	quote! { #item }
}

fn expand_struct(telescope: &[TypeParam], item: &ItemStruct) -> TokenStream {
	let mut item = item.clone();

	// Add telescope type params to generics
	for param in telescope {
		item.generics.params.push(syn::GenericParam::Type(param.clone()));
	}

	// Check if telescope types are used in fields; if not, add PhantomData
	let tele_idents: Vec<&Ident> = telescope.iter().map(|p| &p.ident).collect();
	let needs_phantom = !tele_idents.is_empty() && {
		let fields_tokens = match &item.fields {
			syn::Fields::Named(f) => quote! { #f },
			syn::Fields::Unnamed(f) => quote! { #f },
			syn::Fields::Unit => TokenStream::new(),
		};
		let fields_str = fields_tokens.to_string();
		tele_idents.iter().any(|id| !fields_str.contains(&id.to_string()))
	};

	if needs_phantom {
		if let syn::Fields::Named(ref mut fields) = item.fields {
			let phantom_field: syn::Field = syn::parse_quote! {
				_phantom: core::marker::PhantomData<( #(#tele_idents,)* )>
			};
			fields.named.push(phantom_field);
		}
	}

	quote! { #item }
}

fn expand_impl(telescope: &[TypeParam], item: &ItemImpl) -> TokenStream {
	let mut item = item.clone();
	for param in telescope {
		item.generics.params.push(syn::GenericParam::Type(param.clone()));
	}
	quote! { #item }
}

fn expand_mod(telescope: &[TypeParam], item: &ItemMod) -> TokenStream {
	let attrs = &item.attrs;
	let vis = &item.vis;
	let ident = &item.ident;

	match &item.content {
		Some((_brace, items)) => {
			// Re-parse inner items as ModuleItems so we can recurse
			let inner: Vec<ModuleItem> = items.iter().map(|i| classify_item(i.clone())).collect();
			let expanded = expand_items(telescope, &inner);
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

fn expand_conditional(telescope: &[TypeParam], cond: &Conditional) -> TokenStream {
	let branch = match &cond.condition {
		Some(_) => &cond.true_branch,
		None => &cond.false_branch,
	};
	expand_items(telescope, branch)
}

fn classify_item(item: Item) -> ModuleItem {
	match item {
		Item::Type(t) => ModuleItem::TypeAlias(t),
		Item::Struct(s) => ModuleItem::Struct(s),
		Item::Impl(i) => ModuleItem::Impl(i),
		Item::Mod(m) => ModuleItem::Mod(m),
		other => ModuleItem::Other(other),
	}
}
