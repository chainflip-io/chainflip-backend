use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput};

#[proc_macro_derive(EnumVariant)]
pub fn from_discriminant_derive(item: TokenStream) -> TokenStream {
	let input = parse_macro_input!(item as DeriveInput);

	expand(input)
		.map(Into::into)
		.unwrap_or_else(syn::Error::into_compile_error)
		.into()
}

fn fold_error(left: syn::Error, right: syn::Error) -> syn::Error {
	let mut res = left;
	res.combine(right);
	res
}

fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
	let vis = input.vis.clone();
	let ident = input.ident.clone();
	let ident_naked = format_ident!("{}Variant", ident);
	let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
	let mut errors: Vec<syn::Error> = Default::default();

	let (discriminants, variants) = match input.data {
		Data::Enum(data) => Ok(data
			.variants
			.iter()
			.enumerate()
			.filter_map(|(i, variant)| {
				if variant.discriminant.is_some() {
					errors.push(syn::Error::new_spanned(
						variant.clone(),
						"EnumVariant derive: custom enum discriminants not supported.",
					));
					None
				} else {
					Some((i as u8, variant.ident.clone()))
				}
			})
			.unzip::<_, _, Vec<_>, Vec<_>>()),
		Data::Struct(s) => Err(syn::Error::new_spanned(
			s.struct_token,
			"EnumVariant derive: can only be applied to enums.",
		)),
		Data::Union(u) => Err(syn::Error::new_spanned(
			u.union_token,
			"EnumVariant derive: can only be applied to enums.",
		)),
	}?;

	let num_discriminants = discriminants.len();

	if let Some(e) = errors.into_iter().reduce(fold_error) {
		return Err(e)
	}

	let output = quote!(
		impl #impl_generics EnumVariant for #ident #ty_generics #where_clause {
			type Variant = #ident_naked;

			fn from_discriminant(d: u8) -> Option<Self::Variant> {
				if d as usize >= #num_discriminants {
					return None
				}
				Some(match d {
					#( #discriminants => Self::Variant::#variants, )*
					_ => unreachable!(),
				})
			}
		}

		#[derive(Copy, Clone, Debug, PartialEq, Eq)]
		#vis enum #ident_naked {
			#( #variants, )*
		}
	);

	Ok(output.into())
}
