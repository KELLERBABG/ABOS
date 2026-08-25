use abos_common::error::Result;
use abos_common::types::Bundle;
use sled::Db;

pub struct BundleStore {
    db: Db,
}

impl BundleStore {
    pub fn new(path: &str) -> Result<Self> {
        let db = sled::open(path).map_err(|e| abos_common::error::Error::StorageError(e.to_string()))?;
        Ok(Self { db })
    }

    pub fn store_bundle(&self, bundle: &Bundle) -> Result<()> {
        let value = bincode::serialize(bundle).map_err(|e| abos_common::error::Error::StorageError(e.to_string()))?;
        self.db.insert(&bundle.bundle_id[..], value).map_err(|e| abos_common::error::Error::StorageError(e.to_string()))?;
        Ok(())
    }

    pub fn retrieve_bundle(&self, bundle_id: &[u8; 32]) -> Result<Option<Bundle>> {
        match self.db.get(&bundle_id[..]).map_err(|e| abos_common::error::Error::StorageError(e.to_string()))? {
            Some(ivec) => {
                let bundle: Bundle = bincode::deserialize(&ivec).map_err(|e| abos_common::error::Error::StorageError(e.to_string()))?;
                Ok(Some(bundle))
            }
            None => Ok(None),
        }
    }
}
