use type_vault_trait::*;
use type_vault_trait_derive::VaultType;

use std::any::TypeId;

use serde::{Deserialize, Serialize};
use bincode;

#[derive(Hash, Clone, Deserialize, VaultType)]
struct BaseStruct {
    foo: i32,
}

#[derive(Hash, Clone, Deserialize, VaultType)]
struct UnitStruct;

#[derive(Hash, Clone, Deserialize, VaultType)]
struct UnnamedStruct (i32);

#[derive(Deserialize, VaultType)]
struct TestStruct {
    i32_field: i32,
    f64_field: f64,
    bool_field: bool,
    unit_field: (),
    tuple_field: (i32, f64, bool, ()),
    base_field: BaseStruct,
    unit_struct_field: UnitStruct,
    unnamed_struct_field: UnnamedStruct,
    unnamed_enum_struct_field: TestUnnamedEnum,
    named_enum_field: TestNamedEnum,
}

#[derive(Deserialize, VaultType, Debug, PartialEq)]
enum TestUnnamedEnum {
    A(i32),
    B(f64, bool),
    C(),
}

#[derive(Deserialize, VaultType, Debug, PartialEq)]
enum TestNamedEnum {
    A { x: i32 },
    B { y: f64, z: bool },
}

#[test]
fn test_derive() {
    let test_struct =
        TestStruct {
            i32_field: 42,
            f64_field: 0.1,
            bool_field: true,
            unit_field: (),
            tuple_field: (1, 0.2, false, ()),
            base_field: BaseStruct { foo: 10 },
            unit_struct_field: UnitStruct,
            unnamed_struct_field: UnnamedStruct(7),
            unnamed_enum_struct_field: TestUnnamedEnum::A(99),
            named_enum_field: TestNamedEnum::A { x: 123  },
        };
    let id_map = TypeMap::new(vec![
        TypeId::of::<TestStruct>(),
        TypeId::of::<BaseStruct>(),
        TypeId::of::<UnitStruct>(),
        TypeId::of::<UnnamedStruct>(),
        TypeId::of::<TestUnnamedEnum>(),
        TypeId::of::<TestNamedEnum>(),]);
    let serialized: Vec<(Vec<u8>, ValueId)> = serialize_type(&test_struct, &id_map);
    println!("Serialized: {:?}", serialized);
    assert_eq!(serialized.len(), 6); // One for the struct itself, one for each of the nested structs

    // Test prefix serialization
    assert_eq!(test_struct.serialize_prefix(10, &id_map), serialize_type(&test_struct, &id_map).pop().unwrap().0);
    println!("Prefix with all fields: {:?}", test_struct.serialize_prefix(10, &id_map));

    // Deserialization test
    let table = serialized.iter().cloned().map(|(val,id)| (id,val))
            .collect::<std::collections::HashMap<ValueId, Vec<u8>>>();
    let lookup_id = |value_id| {
        table.get(&value_id).cloned()
    };
    match TestStruct::deserialize_value(&serialized[serialized.len()-1].0, &lookup_id) {
    Some((_,deserialized)) => {
            // Check that the deserialized instance matches the original
            assert_eq!(deserialized.i32_field, test_struct.i32_field);
            assert_eq!(deserialized.f64_field, test_struct.f64_field);
            assert_eq!(deserialized.bool_field, test_struct.bool_field);
            assert_eq!(deserialized.tuple_field, test_struct.tuple_field);
            assert_eq!(deserialized.base_field.foo, test_struct.base_field.foo);
            assert_eq!(deserialized.unnamed_struct_field.0, test_struct.unnamed_struct_field.0);
            assert_eq!(deserialized.unnamed_enum_struct_field, test_struct.unnamed_enum_struct_field);
            assert_eq!(deserialized.named_enum_field, test_struct.named_enum_field);
    },
        None => panic!("Deserialization failed"),
    }
}
