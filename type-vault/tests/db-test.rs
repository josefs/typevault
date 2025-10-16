use type_vault_trait::*;
use type_vault_trait_derive::VaultType;

use std::any::TypeId;

use serde::{Deserialize, Serialize};
use bincode; // For serialization
use type_vault::new_type_vault; // Import FactDB from the appropriate crate

#[derive(VaultType, Debug, PartialEq, Clone)]
struct BaseStruct {
    foo: u32,
}

#[derive(VaultType, Debug, PartialEq, Clone)]
struct TestStruct {
    field: u32,
    base_field: Box<BaseStruct>,
    rec_field: Option<Box<TestStruct>>,
}

#[test]
fn test_db_storage() {
    // Test structs
    let struct1 = TestStruct { field: 42, base_field: Box::new(BaseStruct { foo: 10 }), rec_field : None };
    let struct2 = TestStruct { field: 42, base_field: Box::new(BaseStruct { foo: 20 }), rec_field : None };
    let struct3 = TestStruct { field: 43, base_field: Box::new(BaseStruct { foo: 10 }), rec_field : Some(Box::new(struct1.clone())) };

    // Set up DB.
    let db = new_type_vault!(std::path::Path::new("test_db"), TestStruct, BaseStruct);
    db.clear().unwrap();
    db.put(&struct1).unwrap();
    db.put(&struct2).unwrap();
    db.put(&struct3).unwrap();
    db.debug_print();
    // The first byte 0u8 is the type id for TestStruct, the second byte 42u8 is the value of the `field` field.
    let mut visited = 0;
    db.debug_scan_primitive(vec![0u8, 42u8]).for_each(|(value, id) | {
        println!("Scanned Value with ID {:?}: {:?}", id, value);
        visited += 1;
    });
    assert_eq!(visited, 2); // At least struct1 and struct2 should match

    // Roundtripping
    let serialized: Vec<(Vec<u8>, ValueId)> = serialize_type(& struct1, &db.type_map);
    let lookup_id = |id| {
        for (vec, hash) in serialized.iter() {
            if *hash == id {
                return Some(vec.clone());
            }
        }
        None
    };
    let round_trip: Option<TestStruct> =
      deserialize_type(&serialized[2].0, &lookup_id);
    match round_trip {
        Some(deserialized) => {
            // Check that the deserialized instance matches the original
            assert_eq!(deserialized.field, struct1.field);
            assert_eq!(deserialized.base_field.foo, struct1.base_field.foo);
            println!("Roundtripped successfully: {:?}", deserialized);
        },
        None => panic!("Roundtripping failed"),
    }

    // Full prefix scan test
    let scan_result: Vec<(Box<TestStruct>, ValueId)> =
      db.scan(TestStruct { field: 42, base_field: Box::new(BaseStruct { foo: 0 }), rec_field : None }, 1).collect();
    assert_eq!(scan_result.into_iter().map(|(value, _id)| *value).collect::<Vec<TestStruct>>()
      , vec![struct1, struct2]); //TODO: Make the test robust to ordering
    let scan_result2: Vec<(Box<TestStruct>, ValueId)> =
      db.scan(TestStruct { field: 43, base_field: Box::new(BaseStruct { foo: 10 }), rec_field : None }, 2).collect();
    assert_eq!(scan_result2.into_iter().map(|(value, _id)| *value).collect::<Vec<TestStruct>>()
      , vec![struct3]);

}