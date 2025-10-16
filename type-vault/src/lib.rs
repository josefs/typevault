use type_vault_trait::*;
use std::collections::HashMap;
use std::any::TypeId;

pub struct TypeVault {
  base_db: sled::Db,
  id_to_value_map: sled::Tree,
  value_to_id_map: sled::Tree,
  pub type_ids: TypeMap,
}

#[macro_export]
macro_rules! new_type_vault {
    ($e:expr, $($tys:ty),+) => {
        $crate::TypeVault::new($e, vec![$(std::any::TypeId::of::<$tys>()),+])
    };
}

impl TypeVault {
    pub fn new(path: &std::path::Path, type_ids: Vec<TypeId>) -> Self {
        let db: sled::Db = sled::open(path).expect("Failed to open database");
        let id_to_value_map = db.open_tree("id_to_value").expect("Failed to open id_to_value tree");
        let value_to_id_map = db.open_tree("value_to_id").expect("Failed to open value_to_id tree");
        let mut type_id_map = HashMap::new();
        for (i, type_id) in type_ids.into_iter().enumerate() {
            type_id_map.insert(type_id, i as u8);
        }
        TypeVault { base_db: db
            , id_to_value_map, value_to_id_map
            , type_ids: type_id_map
        }
    }

    pub fn clear(&self) -> Result<(), sled::Error> {
        self.id_to_value_map.clear()?;
        self.value_to_id_map.clear()?;
        Ok(())
    }

    pub fn put<T:VaultType>(&self, value: &T) -> Result<(), sled::Error> {
        let data = serialize_type(value, &self.type_ids);
        for (val, id) in data {
            self.value_to_id_map.insert(&val, &id)?;
            self.id_to_value_map.insert(id, val)?;
        }
        Ok(())
    }

    pub fn scan<'a, T: VaultType>(&'a self, value: T, fields_in_prefix: u64) -> impl Iterator<Item = (Box<T>, ValueId)> + 'a {
        let prefix = value.serialize_prefix(fields_in_prefix, &self.type_ids);
        self.debug_scan(prefix)
    }

    // Shouldn't be public
    pub fn debug_scan<'a, T:VaultType>(&'a self, prefix : Vec<u8>) -> impl Iterator<Item = (Box<T>, ValueId)>  + 'a {
        self.value_to_id_map
            .scan_prefix(prefix)
            //TODO: We want to report an error instead of silently ignoring deserialization failures.
            .filter_map(move |res: Result<(sled::IVec, sled::IVec), sled::Error>| {
                let (data, id_bytes) = res.expect("Failed to read from value_to_id_map");
                let id = bincode::serde::decode_from_slice(&id_bytes, BINCODE_CONFIG).unwrap().0;
                let deserialized = deserialize_type::<T>(&data, &|id_needle| self.lookup_id(id_needle));
                let deserialized = match deserialized {
                    None => {
                        eprintln!("Failed to deserialize data with ID {:?}", id);
                        return None;
                    },
                    Some(d) => d,
                };
                Some ((Box::new(deserialized), id))
            })
    }

    pub fn debug_scan_primitive(&self, prefix: Vec<u8>) -> impl Iterator<Item = (Vec<u8>, ValueId)> {
        self.value_to_id_map
            .scan_prefix(prefix)
            .map(|res: Result<(sled::IVec, sled::IVec), sled::Error>| {
                let (value_data, id_bytes) = res.expect("Failed to read from value_to_id_map");
                let mut id_array = [0u8; 8];
                id_array.copy_from_slice(&id_bytes);
                (value_data.to_vec(), id_array)
            })
    }

    pub fn debug_print(&self) {
        println!("TypeVault contents:\nValue to ID map:");
        for item in self.value_to_id_map.iter() {
            let (key, value) = item.expect("Failed to read from value_to_id_map");
            let mut value_array = [0u8; 8];
            value_array.copy_from_slice(&value);
            println!("Value : {:?}, ID: {:?}", key, value_array);
        }
        println!("ID to Value map:");
        for item in self.id_to_value_map.iter() {
            let (key, value) = item.expect("Failed to read from id_to_value_map");
            let mut key_array = [0u8; 8];
            key_array.copy_from_slice(&key);
            let id = u64::from_be_bytes(key_array);
            println!("ID: {}, Data: {:?}", id, value);
        }
    }

    fn lookup_id(&self, id: ValueId) -> Option<Vec<u8>> {
        if let Ok(Some(id_bytes)) = self.id_to_value_map.get(id) {
            Some(id_bytes.to_vec())
        } else {
            None
        }
    }
}