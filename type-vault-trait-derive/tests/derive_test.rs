use type_vault_trait::*;
use type_vault_trait_derive::VaultType;

use std::{any::TypeId, collections::HashMap};

use serde::{Deserialize, Serialize};
use bincode;

#[derive(Hash, Clone, Deserialize, VaultType)]
struct BaseStruct {
    foo: i32,
}

#[derive(Deserialize, VaultType)]
struct TestStruct {
    i32_field: i32,
    f64_field: f64,
    bool_field: bool,
    base_field: BaseStruct,
}

#[test]
fn test_derive() {
    let test_struct =
        TestStruct {
            i32_field: 42,
            f64_field: 0.1,
            bool_field: true,
            base_field: BaseStruct { foo: 10 }
        };
    let id_map = HashMap::from([
        (std::any::TypeId::of::<TestStruct>(), 1u8),
        (std::any::TypeId::of::<BaseStruct>(), 2u8),
    ]);
    let serialized: Vec<(Vec<u8>, ValueId)> = serialize_type(&test_struct, &id_map);
    println!("Serialized: {:?}", serialized);
    assert_eq!(serialized.len(), 2); // One for the struct itself, one for the base struct

    // Test prefix serialization
    assert_eq!(test_struct.serialize_prefix(4, &id_map), serialize_type(&test_struct, &id_map).pop().unwrap().0);
    println!("Prefix with all fields: {:?}", test_struct.serialize_prefix(4, &id_map));

    // Deserialization test
    let base_serialized: &(Vec<u8>, ValueId) = &serialized[0];
    let lookup_id = |_value_id| {
        // In a real scenario, this would look up the value by its ID.
        // Here, we just return the serialized data for the base struct.
        Some(base_serialized.0.to_vec())
    };
    match TestStruct::deserialize_value(&serialized[1].0, &lookup_id) {
    Some((_,deserialized)) => {
            // Check that the deserialized instance matches the original
            assert_eq!(deserialized.i32_field, test_struct.i32_field);
            assert_eq!(deserialized.f64_field, test_struct.f64_field);
            assert_eq!(deserialized.bool_field, test_struct.bool_field);
            assert_eq!(deserialized.base_field.foo, test_struct.base_field.foo);
    },
        None => panic!("Deserialization failed"),
    }
}
