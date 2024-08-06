use lmdb_rs::{
    Db, DbFlags, Env, EnvBuilder, EnvNoLock, EnvOpenFlags, MdbError, MdbValue, WriteFlags,
};
use std::path::Path;
use std::sync::Mutex;

pub struct LmdbStore {
    env: Env,
    db: Db,
}

impl LmdbStore {
    pub fn new(path: &Path) -> Result<Self, MdbError> {
        let env = EnvBuilder::new().open(path, EnvOpenFlags::empty(), 0o600)?;
        let db = env.get_default_db(DbFlags::empty())?;
        Ok(Self { env, db })
    }

    pub fn insert(&self, key: &[u8], value: &[u8]) -> Result<(), MdbError> {
        let txn = self.env.new_transaction()?;
        txn.put(&self.db, key, value, WriteFlags::empty())?;
        txn.commit()
    }

    pub fn get<'a>(&'a self, key: &[u8]) -> Result<Option<&'a [u8]>, MdbError> {
        let txn = self.env.new_transaction()?;
        let value = txn.get(&self.db, key)?;
        Ok(value)
    }

    pub fn delete(&self, key: &[u8]) -> Result<(), MdbError> {
        let txn = self.env.new_transaction()?;
        txn.del(&self.db, key, None)?;
        txn.commit()
    }
}
