use bincode;
use std::{any::TypeId, collections::HashMap, hash::*};

pub type ValueId = [u8; 8];

pub fn value_id_of(data: impl Hash) -> ValueId {
    let mut s = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut s);
    s.finish().to_be_bytes()
}

pub trait VaultType {
    type InnerVaultType;
    // The reason for multiple Vec<u8> here is to allow for nested structs.
    // The serialized Vec<u8> are in post order, meaning that the nested structs
    // come first and the toplevel structs comes last.
    fn serialize_into(&self, nested_dest: &mut Vec<(Vec<u8>, ValueId)>, dest: &mut Vec<u8>, type_map: &HashMap<TypeId,u8>);
    fn serialize_prefix(&self, fields_in_prefix: u64, type_map: &HashMap<TypeId,u8>) -> Vec<u8>;
    fn deserialize_value<'a>(data: &'a [u8], lookup_id: &dyn Fn (ValueId) -> Option<Vec<u8>>) -> Option<(&'a [u8],Self)> where Self: Sized;
}

// This function is needed because the syntax T::deserialize_value::<T>(...) is not allowed for
// some types of T, such as Box<T>. This function provides a workaround.
pub fn deserialize_type<T: VaultType>(data: &[u8], lookup_id: &dyn Fn (ValueId) -> Option<Vec<u8>>) -> Option<T> {
    T::deserialize_value(&data, lookup_id).map(|(_serialized, val)| val)
}

pub fn serialize_type<T: VaultType>(value: &T, type_map: &HashMap<TypeId,u8>) -> Vec<(Vec<u8>, ValueId)> {
    let mut nested_dest = vec![];
    let mut dest = vec![];
    value.serialize_into(&mut nested_dest, &mut dest, &type_map);
    let id = value_id_of(&dest);
    nested_dest.push((dest, id));
    nested_dest
}

pub const BINCODE_CONFIG: bincode::config::Configuration<bincode::config::BigEndian> =
    bincode::config::standard().with_big_endian();

impl<T: VaultType> VaultType for Box<T> {
    type InnerVaultType = T;
    fn serialize_into(&self, nested_dest: &mut Vec<(Vec<u8>, ValueId)>, dest: &mut Vec<u8>, type_map: &HashMap<TypeId,u8>) {
        (**self).serialize_into(nested_dest, dest, type_map);
    }

    fn serialize_prefix(&self, fields_in_prefix: u64, type_map: &HashMap<TypeId,u8>) -> Vec<u8> {
        (**self).serialize_prefix(fields_in_prefix, type_map)
    }
    fn deserialize_value<'a>(data: &'a [u8], lookup_id: &dyn Fn (ValueId) -> Option<Vec<u8>>) -> Option<(&'a [u8],Self)> where Self: Sized {
        T::deserialize_value(data, lookup_id).map(|(serialized, val)| (serialized, Box::new(val)))
    }
}

impl<T: VaultType, U: VaultType> VaultType for (T,U) {
    type InnerVaultType = U;
    fn serialize_into(&self, nested_dest: &mut Vec<(Vec<u8>, ValueId)>, dest: &mut Vec<u8>, type_map: &HashMap<TypeId,u8>) {
        self.0.serialize_into(nested_dest, dest, type_map);
        self.1.serialize_into(nested_dest, dest, type_map);
    }

    fn serialize_prefix(&self, _fields_in_prefix: u64, _type_map: &HashMap<TypeId,u8>) -> Vec<u8> {
        // This method is only meant for structs.
        panic!("Prefix serialization not supported for tuples");
    }

    fn deserialize_value<'a>(data: &'a [u8], lookup_id: &dyn Fn (ValueId) -> Option<Vec<u8>>) -> Option<(&'a [u8],Self)> where Self: Sized {
        let (more_data, first) =
            match T::deserialize_value(data, &lookup_id) {
                None => {
                    eprintln!("Failed to decode first element of tuple");
                    return None
                },
                Some((more_data, val)) => (more_data, val),
        };
        let (rest, second) =
            match U::deserialize_value(&more_data, lookup_id) {
                None => {
                    eprintln!("Failed to decode second element of tuple");
                    return None
                },
                Some((rest, val)) => (rest, val),
        };
        Some((rest, (first, second)))
    }
}


impl<T: VaultType> VaultType for Option<T> {
    type InnerVaultType = T;
    fn serialize_prefix(&self, _fields_in_prefix: u64, _type_map: &HashMap<TypeId,u8>) -> Vec<u8> {
        panic!("Prefix serialization not supported for Option types");
    }

    fn serialize_into(&self, nested_dest: &mut Vec<(Vec<u8>, ValueId)>, dest: &mut Vec<u8>, type_map: &HashMap<TypeId,u8>) {
        match self {
            Some(inner) => {
                dest.push(1u8); // Prefix with a 1 byte to indicate Some
                inner.serialize_into(nested_dest, dest, type_map);
            },
            None => {
                dest.push(0u8); // Represent None as a zero byte
            },
        }
    }

    fn deserialize_value<'a>(data: &'a [u8], lookup_id: &dyn Fn (ValueId) -> Option<Vec<u8>>) -> Option<(&'a [u8], Self)> where Self: Sized {
        if data.is_empty() {
            eprintln!("Data is empty when trying to deserialize Option");
            return None;
        }
        match data[0] {
            0 => Some((data, None)), // None case
            1 => {
                let (rest, inner) = T::deserialize_value(&data[1..], lookup_id)?;
                Some((rest, Some(inner)))
            },
            _ => {
                eprintln!("Invalid prefix byte {} when deserializing Option", data[0]);
                None
            },
        }
    }
}

impl VaultType for () {
    type InnerVaultType = ();
    fn serialize_into(&self, _nested_dest: &mut Vec<(Vec<u8>, ValueId)>, _dest: &mut Vec<u8>, _type_map: &HashMap<TypeId,u8>) {
        return
    }

    fn serialize_prefix(&self, _fields_in_prefix: u64, _type_map: &HashMap<TypeId,u8>) -> Vec<u8> {
        vec![]
    }

    fn deserialize_value<'a>(data: &'a [u8], _lookup_id: &dyn Fn (ValueId) -> Option<Vec<u8>>) -> Option<(&'a [u8], Self)> where Self: Sized {
        Some((data, ()))
    }
}