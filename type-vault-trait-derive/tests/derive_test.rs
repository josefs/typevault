use type_vault_trait::*;
use type_vault_trait_derive::VaultType;

use std::hash::{DefaultHasher, Hash, Hasher};

use serde::{Deserialize, Serialize};
use bincode;

#[derive(Hash, Clone, Deserialize, VaultType)]
struct BaseStruct {
    foo: i32,
}

#[derive(Deserialize, VaultType)]
struct TestStruct {
    field: i32,
    base_field: BaseStruct,
}

#[test]
fn test_derive() {
    let instance = TestStruct { field: 42, base_field: BaseStruct { foo: 10 } };
    let serialized: Vec<(Vec<u8>, ValueId)> = serialize_type(&instance);
    println!("Serialized: {:?}", serialized);
    assert_eq!(serialized.len(), 2); // One for the struct itself, one for the base struct

    // Test prefix serialization
    assert_eq!(instance.serialize_prefix(2), serialize_type(&instance).pop().unwrap().0); // No fields
    println!("Prefix with 2 fields: {:?}", instance.serialize_prefix(2));
    assert_eq!(instance.serialize_prefix(1),
        bincode::serde::encode_to_vec(instance.field, BINCODE_CONFIG).unwrap()); // One field

    // Deserialization test
    let base_serialized: &(Vec<u8>, u64) = &serialized[0];
    let lookup_id = |_value_id| {
        // In a real scenario, this would look up the value by its ID.
        // Here, we just return the serialized data for the base struct.
        Some(base_serialized.0.to_vec())
    };
    match TestStruct::deserialize_value(&serialized[1].0, &lookup_id) {
        Some((_,deserialized)) => {
            // Check that the deserialized instance matches the original
            assert_eq!(deserialized.field, instance.field);
            assert_eq!(deserialized.base_field.foo, instance.base_field.foo);
    },
        None => panic!("Deserialization failed"),
    }
}
