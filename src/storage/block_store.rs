use std::sync::Arc;

use crate::error::HotStuffError;
use crate::types::{Block, Hash};

pub trait BlockStore: Send + Sync {
    fn put_block(&self, block: &Block) -> Result<(), HotStuffError>;
    fn get_block(&self, hash: &Hash) -> Result<Option<Block>, HotStuffError>;
    fn has_block(&self, hash: &Hash) -> Result<bool, HotStuffError>;
    fn get_latest_committed_block(&self) -> Result<Block, HotStuffError>;
}

pub struct MemoryBlockStore {
    blocks: std::sync::RwLock<std::collections::HashMap<Hash, Block>>,
}

impl MemoryBlockStore {
    pub fn new() -> Self {
        Self {
            blocks: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

impl BlockStore for MemoryBlockStore {
    fn put_block(&self, block: &Block) -> Result<(), HotStuffError> {
        let mut blocks = self.blocks.write().unwrap();
        blocks.insert(block.hash, block.clone());
        Ok(())
    }

    fn get_block(&self, hash: &Hash) -> Result<Option<Block>, HotStuffError> {
        let blocks = self.blocks.read().unwrap();
        Ok(blocks.get(hash).cloned())
    }

    fn has_block(&self, hash: &Hash) -> Result<bool, HotStuffError> {
        let blocks = self.blocks.read().unwrap();
        Ok(blocks.contains_key(hash))
    }

    fn get_latest_committed_block(&self) -> Result<Block, HotStuffError> {
        let blocks = self.blocks.read().unwrap();
        blocks
            .values()
            .max_by_key(|block| block.height)
            .cloned()
            .ok_or(HotStuffError::Storage("No blocks found".to_string()))
    }
}

impl<T: BlockStore + ?Sized> BlockStore for Arc<T> {
    fn put_block(&self, block: &Block) -> Result<(), HotStuffError> {
        (**self).put_block(block)
    }

    fn get_block(&self, hash: &Hash) -> Result<Option<Block>, HotStuffError> {
        (**self).get_block(hash)
    }

    fn has_block(&self, hash: &Hash) -> Result<bool, HotStuffError> {
        (**self).has_block(hash)
    }

    fn get_latest_committed_block(&self) -> Result<Block, HotStuffError> {
        (**self).get_latest_committed_block()
    }
}
