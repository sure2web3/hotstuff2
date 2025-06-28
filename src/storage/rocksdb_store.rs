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

    fn get_latest_committed_block(&self) -> Result<Block, HotStuffError> {
        // For simplicity, this implementation scans all blocks
        // A real implementation would maintain an index of committed blocks
        let mut latest_block: Option<Block> = None;
        let mut latest_height = 0u64;

        let iter = self.db.iterator(rocksdb::IteratorMode::Start);
        for item in iter {
            let (key, value) = item.map_err(|e| HotStuffError::Storage(e.to_string()))?;
            
            if let Ok(key_str) = String::from_utf8(key.to_vec()) {
                if key_str.starts_with("block:") {
                    if let Ok(block) = bincode::deserialize::<Block>(&value) {
                        if block.height > latest_height {
                            latest_height = block.height;
                            latest_block = Some(block);
                        }
                    }
                }
            }
        }

        latest_block.ok_or(HotStuffError::Storage("No blocks found".to_string()))
    }
}
