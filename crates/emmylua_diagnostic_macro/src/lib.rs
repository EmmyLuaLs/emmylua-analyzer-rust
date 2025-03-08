extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, spanned::Spanned, Data, DeriveInput, Fields, Ident};

// Convert enum variant names to kebab-case
fn to_kebab_case(ident: &Ident) -> String {
    let s = ident.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i != 0 {
            result.push('-');
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c.to_ascii_lowercase());
        }
    }
    result
}

#[proc_macro_derive(LuaDiagnosticMacro)]
pub fn lua_diagnostic_macro(input: TokenStream) -> TokenStream {
    // Parse the input TokenStream into DeriveInput
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident.clone();

    // Ensure the input is an enum
    let variants = match input.data {
        Data::Enum(ref data_enum) => &data_enum.variants,
        _ => {
            return syn::Error::new(input.span(), "LuaDiagnosticMacro only supports enums")
                .to_compile_error()
                .into();
        }
    };

    // Generate get_name / FromStr / Display / all based on variants
    let mut variant_idents = Vec::new();
    let mut variant_strings = Vec::new();

    for variant in variants.iter() {
        let variant_ident = &variant.ident;
        // Handle unit variants
        if let Fields::Unit = &variant.fields {
            let kebab_case_string = to_kebab_case(variant_ident);
            variant_idents.push(variant_ident);
            variant_strings.push(kebab_case_string);
        } else {
            // Only unit variants are supported
            return syn::Error::new(variant.ident.span(), "Only unit variants supported")
                .to_compile_error()
                .into();
        }
    }

    // Build match arms for get_name()
    let get_name_arms = variant_idents
        .iter()
        .zip(variant_strings.iter())
        .map(|(ident, kc)| {
            quote! {
                #name::#ident => #kc
            }
        });

    // Build match arms for FromStr
    let from_str_arms = variant_idents
        .iter()
        .zip(variant_strings.iter())
        .map(|(ident, kc)| {
            quote! {
                #kc => Ok(#name::#ident)
            }
        });

    // Build the all() array
    let all_variants = variant_idents.iter().map(|ident| {
        quote! { #name::#ident }
    });

    // Generate the complete impl block
    let expanded = quote! {
        impl #name {
            pub fn get_name(&self) -> &str {
                match self {
                    #(#get_name_arms),*,
                    _ => "none"
                }
            }

            // Return all variants
            pub fn all() -> Vec<#name> {
                vec![
                    #(#all_variants),*
                ]
            }
        }

        impl ::std::str::FromStr for #name {
            type Err = ();

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    #(#from_str_arms),*,
                    _ => Ok(#name::None),
                }
            }
        }

        impl ::std::fmt::Display for #name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}", self.get_name())
            }
        }
    };

    expanded.into()
}
