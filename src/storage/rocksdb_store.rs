use rocksdb::{Options, DB};
use std::path::Path;

use crate::error::HotStuffError;
use crate::storage::BlockStore;
use crate::types::{Block, Hash};

pub struct RocksDBStore {
    db: DB,
}

impl RocksDBStore {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, HotStuffError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let db = DB::open(&opts, path).map_err(|e| HotStuffError::Storage(e.to_string()))?;

        Ok(Self { db })
    }

    pub fn put_quorum_cert(
        &self,
        height: u64,
        qc: &crate::types::QuorumCert,
    ) -> Result<(), HotStuffError> {
        let key = format!("qc:{}", height);
        let value = bincode::serialize(qc)?;
        self.db
            .put(key, value)
            .map_err(|e| HotStuffError::Storage(e.to_string()))
    }

    pub fn get_quorum_cert(
        &self,
        height: u64,
    ) -> Result<Option<crate::types::QuorumCert>, HotStuffError> {
        let key = format!("qc:{}", height);
        match self
            .db
            .get(key)
            .map_err(|e| HotStuffError::Storage(e.to_string()))?
        {
            Some(data) => Ok(Some(bincode::deserialize(&data)?)),
            None => Ok(None),
        }
    }
}

impl BlockStore for RocksDBStore {
    fn put_block(&self, block: &Block) -> Result<(), HotStuffError> {
        let key = format!("block:{}", block.hash());
        let value = bincode::serialize(block)?;
        self.db
            .put(key, value)
            .map_err(|e| HotStuffError::Storage(e.to_string()))
    }

    fn get_block(&self, hash: &Hash) -> Result<Option<Block>, HotStuffError> {
        let key = format!("block:{}", hash);
        match self
            .db
            .get(key)
            .map_err(|e| HotStuffError::Storage(e.to_string()))?
        {
            Some(data) => Ok(Some(bincode::deserialize(&data)?)),
            None => Ok(None),
        }
    }

    fn has_block(&self, hash: &Hash) -> Result<bool, HotStuffError> {
        let key = format!("block:{}", hash);
        match self
            .db
            .get(key)
            .map_err(|e| HotStuffError::Storage(e.to_string()))?
        {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }
}
