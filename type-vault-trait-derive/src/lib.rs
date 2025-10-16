use proc_macro::TokenStream;
use proc_macro2::Literal;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Field, Fields, FieldsNamed, FieldsUnnamed, Type};
use std::iter::zip;

#[proc_macro_derive(VaultType)]
pub fn replace_with_value_id(input: TokenStream) -> TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  let name = input.ident;
  let new_name = syn::Ident::new(&format!("{}Fact", name), name.span());

  match input.data {
    Data::Enum(_) | Data::Union(_) => panic!("ReplaceWithValueId can only be used with structs"),
    Data::Struct(data_struct) => {
      match data_struct.fields {
        Fields::Unit =>
          TokenStream::from(quote! {
            impl VaultType for #name {
              type InnerVaultType = (#name);

              fn serialize_into(&self, _nested_dest: &mut Vec<(Vec<u8>, ValueId)>, dest: &mut Vec<u8>, type_map: &TypeMap) {
                dest.append(&mut type_map.get(&TypeId::of::<Self>()).expect("Type not registered in type map").to_owned());
              }

              fn serialize_prefix(&self, fields_in_prefix: u64, type_map: &TypeMap) -> Vec<u8> {
                return type_map.get(&TypeId::of::<Self>()).expect("Type not registered in type map").to_owned();
              }

              fn deserialize_value<'a>(data: &'a [u8], _lookup_id: &dyn Fn(ValueId) -> Option<Vec<u8>>) -> Option<(&'a [u8],Self)> where Self: Sized {
                Some((&data[1..], Self {}))
              }
            }
          }
          ),

        Fields::Unnamed(unnamed_fields) => {
          let NewUnnamedFieldsInfo {
            field_indices,
            field_types,
            field_vars,
            is_modified_field,
            modified_field_indices,
            modified_field_types,
            unmodified_field_indices,
          } = convert_unnamed_fields(&unnamed_fields);

          let serialize_into_fields = zip(&field_indices, zip(&is_modified_field, &field_vars)).map(|(field_index,(is_modified, field_var))| {
            if *is_modified {
              quote! {
                let mut dest_nested = vec![];
                self.#field_index.serialize_into(nested_dest, &mut dest_nested, type_map);
                let #field_var = value_id_of(&dest_nested);
                nested_dest.push((dest_nested, #field_var));
              }
            } else {
              quote! {
                let #field_var = self.#field_index;
              }
            }
          });

          let serialize_fields = zip(&field_indices, zip(&is_modified_field, &field_vars)).map(|(field_index,(is_modified, field_var))| {
            if *is_modified {
              quote! {
                let mut dest = vec![];
                // Ignore the nested fields. We only care about the hash.
                self.#field_index.serialize_into(&mut vec![], &mut dest, type_map);
                let #field_var = value_id_of(&dest);
                result.extend(bincode::serde::encode_to_vec(#field_var, BINCODE_CONFIG).unwrap());
              }
            } else {
              quote! {
                let #field_var = self.#field_index;
                result.extend(bincode::serde::encode_to_vec(#field_var, BINCODE_CONFIG).unwrap());
              }
            }
          });

          let deserialize_fields = zip(&is_modified_field, zip(&field_vars, zip(&field_types, &field_indices))).map(|(is_modified, (field_var, (field_type, field_index)))| {
            if *is_modified {
            quote! {
              let nested_data = match lookup_id(new_struct.#field_index) {
                None => {
                  eprintln!("Failed to look up ID {:?} for field {} of struct {}", new_struct.#field_index, stringify!(#field_index), stringify!(#name));
                  return None
                },
                Some(data) => data,
              };
              let #field_var : #field_type = deserialize_type::<#field_type>(&nested_data, lookup_id)?;
            }
          } else {
            quote! {
              let #field_var = new_struct.#field_index;
            }
          }});

          TokenStream::from(quote!{
            #[derive(Serialize, Deserialize)]
            pub struct #new_name (
              #(#field_types),*
            );

            impl VaultType for #name {
              type InnerVaultType = (#new_name #(,#modified_field_types)*);

              fn serialize_into(&self, nested_dest: &mut Vec<(Vec<u8>, ValueId)>, dest: &mut Vec<u8>, type_map: &TypeMap) {
                #(
                  #serialize_into_fields
                )*
                let strct = #new_name (
                  #(#field_vars),*,
                );
                let mut serialized = match bincode::serde::encode_to_vec(&strct, BINCODE_CONFIG) {
                  Ok(vec) => vec,
                  Err(err) => panic!("bincode failed with: {:?}", err),
                };
                dest.append(&mut type_map.get(&TypeId::of::<Self>()).expect("Type not registered in type map").to_owned());
                dest.append(&mut serialized);

              }

              fn serialize_prefix(&self, fields_in_prefix: u64, type_map: &TypeMap) -> Vec<u8> {
                let mut result = type_map.get(&TypeId::of::<Self>()).expect("Type not registered in type map").to_owned();
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
                  #deserialize_fields
                )*
                Some((
                  &data[bytes_consumed..],
                  Self (
                    #(#field_vars),*
                  )
                ))
              }

            }
            }
          )
        },

        Fields::Named(named_fields) => {
          let NewNamedFieldsInfo {
            field_names,
            field_types,
            field_vars,
            is_modified_field,
            modified_fields,
            modified_field_types,
            unmodified_fields,
          } = convert_named_fields(&named_fields);

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

          TokenStream::from(quote! {
            #[derive(Serialize, Deserialize)]
              pub struct #new_name {
              #(#field_names : #field_types),*
             }

            impl VaultType for #name {
              type InnerVaultType = (#new_name #(,#modified_field_types)*);

              fn serialize_into(&self, nested_dest: &mut Vec<(Vec<u8>, ValueId)>, dest: &mut Vec<u8>, type_map: &TypeMap) {
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
                dest.append(&mut type_map.get(&TypeId::of::<Self>()).expect("Type not registered in type map").to_owned());
                dest.append(&mut serialized);
              }

              fn serialize_prefix(&self, fields_in_prefix: u64, type_map: &TypeMap) -> Vec<u8> {
                let mut result = type_map.get(&TypeId::of::<Self>()).expect("Type not registered in type map").to_owned();
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
                  let #modified_fields : #modified_field_types = deserialize_type::<#modified_field_types>(&nested_data, lookup_id)?;
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
          )
        }
      }
    }
  }
}

fn is_primitive_type(ty: &Type) -> bool {
  match ty {
    Type::Path(ref type_path)
      if type_path.path.is_ident("u8")
      || type_path.path.is_ident("u16")
      || type_path.path.is_ident("u32")
      || type_path.path.is_ident("u64")
      || type_path.path.is_ident("u128")
      || type_path.path.is_ident("i8")
      || type_path.path.is_ident("i16")
      || type_path.path.is_ident("i32")
      || type_path.path.is_ident("i64")
      || type_path.path.is_ident("i128")
      || type_path.path.is_ident("f32")
      || type_path.path.is_ident("f64")
      || type_path.path.is_ident("bool") => return true,
    Type::Array(el_ty) => return is_primitive_type(&el_ty.elem),
    Type::Tuple(tuple) => {
      for elem in &tuple.elems {
        if !is_primitive_type(elem) {
          return false;
        }
      }
      return true;
    },
    _ => return false,
  }
}

enum FieldsInfo {
  Named(NewNamedFieldsInfo),
  Unnamed(NewUnnamedFieldsInfo),
  Unit,
}
struct NewNamedFieldsInfo {
  field_names: Vec<syn::Ident>,
  field_types: Vec<Type>,
  field_vars: Vec<syn::Ident>,
  is_modified_field: Vec<bool>,
  modified_fields: Vec<syn::Ident>,
  modified_field_types: Vec<Type>,
  unmodified_fields: Vec<syn::Ident>,
}

struct NewUnnamedFieldsInfo {
  field_indices: Vec<Literal>,
  field_types: Vec<Type>,
  field_vars: Vec<syn::Ident>,
  is_modified_field: Vec<bool>,
  modified_field_indices: Vec<Literal>,
  modified_field_types: Vec<Type>,
  unmodified_field_indices: Vec<Literal>,
}

fn convert_named_fields(named_fields: &FieldsNamed) -> NewNamedFieldsInfo {
  let mut field_names = Vec::new();
  let mut field_types = Vec::new();
  let mut field_vars = Vec::new();
  let mut is_modified_field = Vec::new();
  let mut modified_fields = Vec::new();
  let mut modified_field_types = Vec::new();
  let mut unmodified_fields = Vec::new();

  for field in &named_fields.named {
    let ty: &Type = &field.ty;
    field_names.push(field.ident.as_ref().unwrap().clone());
    field_vars.push(syn::Ident::new(&format!("var_{}", field.ident.as_ref().unwrap()), field.ident.as_ref().unwrap().span()));
    if is_primitive_type(ty) {
      field_types.push(ty.clone());
      is_modified_field.push(false);
      unmodified_fields.push(field.ident.as_ref().unwrap().clone());
    } else {
      field_types.push(syn::parse_quote!(ValueId));
      is_modified_field.push(true);
      modified_fields.push(field.ident.as_ref().unwrap().clone());
      modified_field_types.push(ty.clone());
    }
  }
  NewNamedFieldsInfo {
    field_names,
    field_types,
    field_vars,
    is_modified_field,
    modified_fields,
    modified_field_types,
    unmodified_fields,
  }
}

fn convert_unnamed_fields(unnamed_fields: &FieldsUnnamed) -> NewUnnamedFieldsInfo {
  let mut field_indices = Vec::new();
  let mut field_types = Vec::new();
  let mut field_vars = Vec::new();
  let mut is_modified_field = Vec::new();
  let mut modified_field_indices = Vec::new();
  let mut modified_field_types = Vec::new();
  let mut unmodified_field_indices = Vec::new();

  for (i, field) in unnamed_fields.unnamed.iter().enumerate() {
    let ty: &Type = &field.ty;
    // Don't know if the span here is correct. But I don't think it matters much.
    // let index = syn::Ident::new(&format!("{}", i), proc_macro2::Span::call_site());
    let index = proc_macro2::Literal::usize_unsuffixed(i);
    field_indices.push(index.clone());
    field_vars.push(syn::Ident::new(&format!("field_{}", i), proc_macro2::Span::call_site()));
    if is_primitive_type(ty) {
      field_types.push(ty.clone());
      is_modified_field.push(false);
      unmodified_field_indices.push(index);
    } else {
      field_types.push(syn::parse_quote!(ValueId));
      is_modified_field.push(true);
      modified_field_types.push(ty.clone());
      modified_field_indices.push(index);
    }
  }
  NewUnnamedFieldsInfo {
    field_indices,
    field_types,
    field_vars,
    is_modified_field,
    modified_field_indices,
    modified_field_types,
    unmodified_field_indices,
  }

}
