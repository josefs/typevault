use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Data, Fields, Type};


#[proc_macro_derive(VaultType)]
pub fn replace_with_value_id(input: TokenStream) -> TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  let name = input.ident;
  let new_name = syn::Ident::new(&format!("{}Fact", name), name.span());

  let expanded = if let Data::Struct(data_struct) = input.data {
    let mut types = Vec::new();
    let mut unmodified_fields = Vec::new();
    let mut modified_fields = Vec::new();
    let mut modified_field_types = Vec::new();
    let mut is_modified_field: Vec<bool> = Vec::new();
    let fields: Vec<_> = match data_struct.fields {
      Fields::Named(fields_named) => fields_named.named.into_iter().map(|mut field| {
        if !matches!(
          field.ty,
          Type::Path(ref type_path) if type_path.path.is_ident("i32")
            || type_path.path.is_ident("i64")
            || type_path.path.is_ident("u32")
            || type_path.path.is_ident("u64")
        ) {
          types.push(field.ty.clone());
          modified_fields.push(field.ident.as_ref().unwrap().clone());
          modified_field_types.push(field.ty.clone());
          field.ty = syn::parse_quote!(ValueId);
          is_modified_field.push(true);
        } else {
          unmodified_fields.push(field.ident.as_ref().unwrap().clone());
          is_modified_field.push(false);
        }
        field
      }).collect(),
      _ => panic!("Only named fields are supported"),
    };

    let field_names =
          fields.iter().map(|field|
            field.ident.as_ref().unwrap()).collect::<Vec<_>>();

    let modified_fields_hash = modified_fields.iter().map(|field| {
      let field_str = field.to_string();
      proc_macro2::Ident::new(&format!("{}_hash", field_str), field.span())
    }).collect::<Vec<_>>();

    let serialize_fields = field_names.iter().zip(is_modified_field.iter()).map(|(field_name, is_modified)| {
      if *is_modified {
        quote! {
          let mut dest = vec![];
          // Ignore the nested fields. We only care about the hash.
          self.#field_name.serialize_into(&mut vec![], &mut dest, type_map);
          let #field_name = value_id_of(&dest);
          result.extend(bincode::serde::encode_to_vec(#field_name, BINCODE_CONFIG).unwrap());
        }
      } else {
        quote! {
          result.extend(bincode::serde::encode_to_vec(self.#field_name, BINCODE_CONFIG).unwrap());
        }
      }
    });

    quote! {
      #[derive(Serialize, Deserialize)]
      pub struct #new_name {
        #(#fields),*
      }

      impl VaultType for #name {
        type InnerVaultType = (#new_name #(,#types)*);


        fn serialize_into(&self, nested_dest: &mut Vec<(Vec<u8>, ValueId)>, dest: &mut Vec<u8>, type_map: &HashMap<std::any::TypeId,u8>) {
          #(
            let mut dest_nested = vec![];
            self.#modified_fields.serialize_into(nested_dest, &mut dest_nested, type_map);
            let #modified_fields_hash = value_id_of(&dest_nested);
            nested_dest.push((dest_nested, #modified_fields_hash));
          )*
          let strct = #new_name {
            #(#unmodified_fields : self.#unmodified_fields),*,
            #(#modified_fields : #modified_fields_hash),*
          };
          let mut serialized = match bincode::serde::encode_to_vec(&strct, BINCODE_CONFIG) {
            Ok(vec) => vec,
            Err(err) => panic!("bincode failed with: {:?}", err),
          };
          dest.append(&mut vec![type_map.get(&TypeId::of::<Self>()).expect("Type not registered in type map").to_owned()]);
          dest.append(&mut serialized);
        }

        fn serialize_prefix(&self, fields_in_prefix: u64, type_map: &HashMap<std::any::TypeId,u8>) -> Vec<u8> {
          let mut result = vec![type_map.get(&TypeId::of::<Self>()).expect("Type not registered in type map").to_owned()];
          let mut remaining_fields = fields_in_prefix;

          #(
            if remaining_fields > 0 {
              #serialize_fields
            } else {
              return result;
            }
            remaining_fields -= 1;
          )*

          result
        }

        fn deserialize_value<'a>(data: &'a [u8], lookup_id: &dyn Fn(ValueId) -> Option<Vec<u8>>) -> Option<(&'a [u8],Self)> where Self: Sized {
          let (new_struct, bytes_consumed): (#new_name, _) =
            //TODO: Check that the type ID matches
            match bincode::serde::decode_from_slice(&data[1..], BINCODE_CONFIG) {
              Err(_) => {
                eprintln!("Failed to decode struct of type {}, data: {:?}", stringify!(#new_name), &data);
                return None
              },
              Ok((strct, bytes_consumed)) => (strct, bytes_consumed),
          };
          #(
            let nested_data = match lookup_id(new_struct.#modified_fields) {
              None => {
                eprintln!("Failed to look up ID {:?} for field {} of struct {}", new_struct.#modified_fields, stringify!(#modified_fields), stringify!(#name));
                return None
              },
              Some(data) => data,
            };
            let #modified_fields : #types = deserialize_type::<#types>(&nested_data, lookup_id)?;
          )*
          Some((
            &data[bytes_consumed..],
            Self {
            #(#unmodified_fields : new_struct.#unmodified_fields),*,
            #(#modified_fields : #modified_fields),*
          }))
        }
      }
    }
  } else {
    panic!("ReplaceWithValueId can only be used with structs");
  };

  TokenStream::from(expanded)
}
