use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Data, Fields};
use proc_macro_crate::{crate_name, FoundCrate};

// Procedural macro to derive `CallbackData` for enums.
// It serializes the enum variant along with a prefix into MessagePack,
// encodes it in Base64, and provides methods for serialization and deserialization.
// The prefix is used as a form of session management to detect invalid data.
#[proc_macro_derive(CallbackData)]
pub fn callback_data_derive(input: TokenStream) -> TokenStream {
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

    // Find the correct path for serde in the user's crate
    let serde_path = match crate_name("serde") {
        Ok(FoundCrate::Itself) => quote! { serde },
        Ok(FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
            quote! { #ident }
        },
        Err(_) => {
            return syn::Error::new_spanned(
                enum_name,
                "Could not find crate `serde`. Please add it to your Cargo.toml.",
            )
                .to_compile_error()
                .into();
        }
    };

    // Find the correct path for rmp-serde in the user's crate
    let rmp_ser_path = match crate_name("rmp-serde") {
        Ok(FoundCrate::Itself) => quote! { rmp_serde },
        Ok(FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
            quote! { #ident }
        },
        Err(_) => {
            return syn::Error::new_spanned(
                enum_name,
                "Could not find crate `rmp-serde`. Please add it to your Cargo.toml.",
            )
                .to_compile_error()
                .into();
        }
    };

    // Find the correct path for base64
    let base64_path = match crate_name("base64") {
        Ok(FoundCrate::Itself) => quote! { base64 },
        Ok(FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
            quote! { #ident }
        },
        Err(_) => {
            return syn::Error::new_spanned(
                enum_name,
                "Could not find crate `base64`. Please add it to your Cargo.toml.",
            )
                .to_compile_error()
                .into();
        }
    };

    // Generate a unique helper struct name based on the enum name
    let helper_struct_name = syn::Ident::new(
        &format!("__CallbackPayload_{}", enum_name),
        proc_macro2::Span::call_site(),
    );

    // Define the helper struct with a unique name and fully qualified serde traits
    let helper_struct = quote! {
        #[derive(::#serde_path::Serialize, ::#serde_path::Deserialize)]
        struct #helper_struct_name<T> {
            prefix: String,
            data: T,
        }
    };

    // Prepare vectors to hold match arms for serialization
    let mut serialize_arms = Vec::new();

    for variant in variants {
        let variant_name = &variant.ident;

        match &variant.fields {
            Fields::Unit => {
                // Variants without fields
                serialize_arms.push(quote! {
                    #enum_name::#variant_name => {
                        // Create payload
                        let payload = #helper_struct_name {
                            prefix: prefix.to_string(),
                            data: #enum_name::#variant_name,
                        };
                        // Serialize using MessagePack
                        let serialized = #rmp_ser_path::to_vec(&payload).expect("Serialization failed");
                        // Encode to Base64
                        #base64_path::encode(&serialized)
                    }
                });
            },
            Fields::Named(_) | Fields::Unnamed(_) => {
                // Variants with named or unnamed fields
                serialize_arms.push(quote! {
                    #enum_name::#variant_name { .. } => {
                        // Create payload
                        let payload = #helper_struct_name {
                            prefix: prefix.to_string(),
                            data: self.clone(),
                        };
                        // Serialize using MessagePack
                        let serialized = #rmp_ser_path::to_vec(&payload).expect("Serialization failed");
                        // Encode to Base64
                        #base64_path::encode(&serialized)
                    }
                });
            },
        }
    }

    // Generate the implementation
    let expanded = quote! {
        #helper_struct

        impl #enum_name {
            // Serializes the enum into a callback data string with the given prefix.
            pub fn to_callback_data(&self, prefix: &str) -> String {
                let serialized_data = match self {
                    #( #serialize_arms ),*
                };
                serialized_data
            }

            // Deserializes a callback data string into the corresponding enum variant.
            // It verifies that the embedded prefix matches the expected prefix.
            pub fn from_callback_data(data: &str, expected_prefix: &str) -> Option<Self> {
                // Decode Base64
                let decoded = #base64_path::decode(data).ok()?;
                // Deserialize the payload using MessagePack
                let payload: #helper_struct_name<#enum_name> = #rmp_ser_path::from_slice(&decoded).ok()?;
                // Verify the prefix
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
