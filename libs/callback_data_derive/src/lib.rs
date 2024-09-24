use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Data, Fields};
use proc_macro_crate::{crate_name, FoundCrate};

// Procedural macro to derive `CallbackData` for enums.
// It serializes the enum variant along with a prefix into MessagePack,
// encodes it in Base64, and provides methods for serialization and deserialization.
// The prefix is used as a form of session management to detect invalid data.
#[proc_macro_derive(CallbackData)]
pub fn callback_data_handler_derive(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);
    let enum_name = input.ident;

    // Ensure the input is an enum
    let variants = match input.data {
        Data::Enum(ref data_enum) => &data_enum.variants,
        _ => {
            return syn::Error::new_spanned(
                enum_name,
                "CallbackData can only be derived for enums",
            )
                .to_compile_error()
                .into();
        }
    };

    // Helper function to find crate paths
    fn find_crate_path(crate_name_str: &str) -> Result<proc_macro2::TokenStream, syn::Error> {
        match crate_name(crate_name_str) {
            Ok(FoundCrate::Itself) => Ok(quote! { #crate_name_str }),
            Ok(FoundCrate::Name(name)) => {
                let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
                Ok(quote! { #ident })
            },
            Err(_) => Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("Could not find crate `{}`. Please add it to your Cargo.toml.", crate_name_str),
            )),
        }
    }

    // Find paths for serde, rmp-serde, and base64
    let serde_path = match find_crate_path("serde") {
        Ok(path) => path,
        Err(err) => return err.to_compile_error().into(),
    };

    let rmp_ser_path = match find_crate_path("rmp-serde") {
        Ok(path) => path,
        Err(err) => return err.to_compile_error().into(),
    };

    let base64_path = match find_crate_path("base64") {
        Ok(path) => path,
        Err(err) => return err.to_compile_error().into(),
    };

    // Find the path to the CallbackDataHandler trait in the prelude crate (`callback_data`)
    let prelude_crate = match crate_name("callback_data") {
        Ok(FoundCrate::Itself) => quote! { callback_data },
        Ok(FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
            quote! { #ident }
        },
        Err(_) => {
            return syn::Error::new_spanned(
                enum_name,
                "Could not find crate `callback_data`. Please add it to your Cargo.toml.",
            )
                .to_compile_error()
                .into();
        }
    };

    let trait_path = quote! { ::#prelude_crate::CallbackDataHandler };

    // Generate a unique helper struct name based on the enum name
    let helper_struct_name = syn::Ident::new(
        &format!("__CallbackPayload_{}", enum_name),
        proc_macro2::Span::call_site(),
    );

    // Define the helper struct with serde traits
    let helper_struct = quote! {
        #[derive(::#serde_path::Serialize, ::#serde_path::Deserialize)]
        struct #helper_struct_name<T> {
            prefix: String,
            data: T,
        }
    };

    // Prepare match arms for serialization
    let mut serialize_arms = Vec::new();

    for variant in variants {
        let variant_name = &variant.ident;

        match &variant.fields {
            Fields::Unit => {
                // Variants without fields
                serialize_arms.push(quote! {
                    #enum_name::#variant_name => {
                        let payload = #helper_struct_name {
                            prefix: prefix.to_string(),
                            data: #enum_name::#variant_name,
                        };
                        let serialized = #rmp_ser_path::to_vec(&payload).expect("Serialization failed");
                        #base64_path::encode(&serialized)
                    }
                });
            },
            Fields::Named(_) | Fields::Unnamed(_) => {
                // Variants with fields
                serialize_arms.push(quote! {
                    variant => {
                        let payload = #helper_struct_name {
                            prefix: prefix.to_string(),
                            data: variant.clone(),
                        };
                        let serialized = #rmp_ser_path::to_vec(&payload).expect("Serialization failed");
                        #base64_path::encode(&serialized)
                    }
                });
            },
        }
    }

    // Generate the implementation of the trait
    let expanded = quote! {
        #helper_struct

        impl #trait_path for #enum_name {
            fn to_callback_data(&self, prefix: &str) -> String {
                match self {
                    #( #serialize_arms ),*
                }
            }

            fn from_callback_data(data: &str, expected_prefix: &str) -> Option<Self> {
                let decoded = #base64_path::decode(data).ok()?;
                let payload: #helper_struct_name<#enum_name> = #rmp_ser_path::from_slice(&decoded).ok()?;
                if payload.prefix == expected_prefix {
                    Some(payload.data)
                } else {
                    None
                }
            }
        }
    };

    TokenStream::from(expanded)
}