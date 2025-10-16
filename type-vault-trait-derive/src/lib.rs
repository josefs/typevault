use proc_macro2::TokenStream;
use quote::quote;
use syn::*;
use std::iter::zip;

#[proc_macro_derive(VaultType)]
pub fn replace_with_value_id(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  let name = input.ident;
  let new_name = Ident::new(&format!("{}Fact", name), name.span());

  match input.data {
    Data::Enum(_) | Data::Union(_) => panic!("ReplaceWithValueId can only be used with structs"),
    Data::Struct(data_struct) => {
      match data_struct.fields {
        Fields::Unit =>
          proc_macro::TokenStream::from(quote! {
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
            field_members,
            field_types,
            field_vars,
            is_modified_field,
            modified_field_types,
            make_struct,
            assign_new_struct,
            build_struct,
          } = convert_unnamed_fields(&new_name, &unnamed_fields);

          create_vault_type_instance_for_struct(&name,new_name, make_struct, &field_vars, field_types, &field_members,
            &is_modified_field,
            modified_field_types, assign_new_struct,
            build_struct)

        },

        Fields::Named(named_fields) => {
          let NewNamedFieldsInfo {
            field_members,
            field_types,
            field_vars,
            is_modified_field,
            modified_field_types,
            make_struct,
            assign_new_struct,
            build_struct,
          } = convert_named_fields(&new_name, &named_fields);

          create_vault_type_instance_for_struct(&name,new_name, make_struct, &field_vars, field_types, &field_members,
            &is_modified_field,
            modified_field_types, assign_new_struct,
            build_struct)
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

fn create_vault_type_instance_for_struct(
    name: &Ident,
    new_name: Ident,
    make_struct: TokenStream,
    field_vars: &Vec<Ident>,
    field_types: Vec<Type>,
    field_members: &Vec<Member>,
    is_modified_field: &Vec<bool>,
    modified_field_types: Vec<Type>,
    assign_new_struct: TokenStream,
    build_struct: TokenStream
  )
    -> proc_macro::TokenStream {

  let serialize_into_fields = zip(field_members, zip(is_modified_field, field_vars)).map(|(field_member, (is_modified, field_var))| {
    if *is_modified {
      quote! {
        let mut dest_nested = vec![];
        self.#field_member.serialize_into(nested_dest, &mut dest_nested, type_map);
        let #field_var = value_id_of(&dest_nested);
        nested_dest.push((dest_nested, #field_var));
      }
    } else {
      quote! {
        let #field_var = self.#field_member;
      }
    }
  });


  let serialize_fields = zip(field_members, is_modified_field).map(|(field_member, is_modified)| {
    if *is_modified {
      quote! {
        let mut dest = vec![];
        // Ignore the nested fields. We only care about the hash.
        self.#field_member.serialize_into(&mut vec![], &mut dest, type_map);
        result.extend(bincode::serde::encode_to_vec(value_id_of(&dest), BINCODE_CONFIG).unwrap());
      }
    } else {
      quote! {
        result.extend(bincode::serde::encode_to_vec(self.#field_member, BINCODE_CONFIG).unwrap());
      }
    }
  });


  let deserialize_fields = zip(is_modified_field, zip(field_vars, zip(&field_types, field_members))).map(|(is_modified, (field_var, (field_type, field_member)))| {
    if *is_modified {
    quote! {
      let nested_data = match lookup_id(new_struct.#field_member) {
        None => {
          eprintln!("Failed to look up ID {:?} for field {} of struct {}", new_struct.#field_member, stringify!(#field_member), stringify!(#name));
          return None
        },
        Some(data) => data,
      };
      let #field_var : #field_type = deserialize_type::<#field_type>(&nested_data, lookup_id)?;
    }
  } else {
    quote! {
      let #field_var = new_struct.#field_member;
    }
  }});

  proc_macro::TokenStream::from(quote! {
    #make_struct

    impl VaultType for #name {
      type InnerVaultType = (#new_name #(,#modified_field_types)*);

      fn serialize_into(&self, nested_dest: &mut Vec<(Vec<u8>, ValueId)>, dest: &mut Vec<u8>, type_map: &TypeMap) {
        #(
          #serialize_into_fields
        )*
        let strct = #assign_new_struct;
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
          #build_struct
        ))
      }
    }
  }
  )
}

enum FieldsInfo {
  Named(NewNamedFieldsInfo),
  Unnamed(NewUnnamedFieldsInfo),
  Unit,
}
struct NewNamedFieldsInfo {
  field_members: Vec<Member>,
  field_types: Vec<Type>,
  field_vars: Vec<Ident>,
  is_modified_field: Vec<bool>,
  modified_field_types: Vec<Type>,
  make_struct: TokenStream,
  assign_new_struct: TokenStream,
  build_struct: TokenStream,
}

struct NewUnnamedFieldsInfo {
  field_members: Vec<Member>,
  field_types: Vec<Type>,
  field_vars: Vec<Ident>,
  is_modified_field: Vec<bool>,
  modified_field_types: Vec<Type>,
  make_struct: TokenStream,
  assign_new_struct: TokenStream,
  build_struct: TokenStream,
}

fn convert_named_fields(new_name: &Ident, named_fields: &FieldsNamed) -> NewNamedFieldsInfo {
  let mut field_members = Vec::new();
  let mut field_types = Vec::new();
  let mut field_vars = Vec::new();
  let mut is_modified_field = Vec::new();
  let mut modified_fields = Vec::new();
  let mut modified_field_types = Vec::new();
  let mut unmodified_fields = Vec::new();

  let mut new_field_types = Vec::new();

  for field in &named_fields.named {
    let ty: &Type = &field.ty;
    let field_member = Member::Named(field.ident.as_ref().unwrap().clone());
    field_members.push(field_member);
    field_types.push(ty.clone());
    field_vars.push(syn::Ident::new(&format!("var_{}", field.ident.as_ref().unwrap()), field.ident.as_ref().unwrap().span()));
    if is_primitive_type(ty) {
      new_field_types.push(ty.clone());
      is_modified_field.push(false);
      unmodified_fields.push(field.ident.as_ref().unwrap().clone());
    } else {
      new_field_types.push(syn::parse_quote!(ValueId));
      is_modified_field.push(true);
      modified_fields.push(field.ident.as_ref().unwrap().clone());
      modified_field_types.push(ty.clone());
    }
  }
  let make_struct: TokenStream = quote!{
      #[derive(Serialize, Deserialize)]
      pub struct #new_name {
        #(#field_members : #new_field_types),*
      }
  };
  let assign_new_struct: TokenStream = quote!{
    #new_name {
      #(#field_members : #field_vars),*
    }
  };
  let build_struct = quote!{
    Self {
      #(#field_members : #field_vars),*
    }
  };
  NewNamedFieldsInfo {
    field_members,
    field_types,
    field_vars,
    is_modified_field,
    modified_field_types,
    make_struct,
    assign_new_struct,
    build_struct,
  }
}

fn convert_unnamed_fields(new_name: &Ident, unnamed_fields: &FieldsUnnamed) -> NewUnnamedFieldsInfo {
  let mut field_members = Vec::new();
  let mut field_types = Vec::new();
  let mut field_vars = Vec::new();
  let mut is_modified_field = Vec::new();
  let mut modified_field_indices = Vec::new();
  let mut modified_field_types = Vec::new();
  let mut unmodified_field_indices = Vec::new();

  let mut new_field_types = Vec::new();

  for (i, field) in unnamed_fields.unnamed.iter().enumerate() {
    let ty: &Type = &field.ty;
    // Don't know if the span here is correct. But I don't think it matters much.
    let index = Member::Unnamed(Index{index: i as u32,
      span: proc_macro2::Span::call_site()});
    field_members.push(index.clone());
    field_types.push(ty.clone());
    field_vars.push(syn::Ident::new(&format!("field_{}", i), proc_macro2::Span::call_site()));
    if is_primitive_type(ty) {
      new_field_types.push(ty.clone());
      is_modified_field.push(false);
      unmodified_field_indices.push(index);
    } else {
      new_field_types.push(syn::parse_quote!(ValueId));
      is_modified_field.push(true);
      modified_field_types.push(ty.clone());
      modified_field_indices.push(index);
    }
  }
  let make_struct = quote!{
    #[derive(Serialize, Deserialize)]
    pub struct #new_name (
      #(#field_types),*
    );
  };
  let assign_new_struct = quote!{
    #new_name(
      #(#field_vars),*
    )
  };
  let build_struct = quote!{
    Self (
      #(#field_vars),*
    )
  };
  NewUnnamedFieldsInfo {
    field_members,
    field_types,
    field_vars,
    is_modified_field,
    modified_field_types,
    make_struct,
    assign_new_struct,
    build_struct,
  }

}

