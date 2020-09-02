use std::{marker::PhantomData, sync::Arc};
use wasm_bindgen::{prelude::*, JsCast};

use crate::{
    object_store::{KeyPath, ObjectStore, ObjectStoreDuringUpgrade},
    request::IdbOpenDbRequest,
    transaction::{Transaction, TransactionMode},
};

#[inline]
fn factory() -> web_sys::IdbFactory {
    web_sys::window().unwrap().indexed_db().unwrap().unwrap()
}

/// A handle on the database during an upgrade.
#[derive(Debug)]
pub struct DbDuringUpgrade {
    db: IndexedDb,
    request: Arc<web_sys::IdbOpenDbRequest>,
}

impl DbDuringUpgrade {
    pub(crate) fn from_raw_unchecked(
        raw: JsValue,
        request: Arc<web_sys::IdbOpenDbRequest>,
    ) -> Self {
        let db = IndexedDb {
            inner: Arc::new(web_sys::IdbDatabase::unchecked_from_js(raw)),
        };
        DbDuringUpgrade { db, request }
    }

    /// Get the name of this database.
    pub fn name(&self) -> String {
        self.db.name()
    }

    /// The current version of the database.
    pub fn version(&self) -> u64 {
        self.db.version()
    }

    /// Create a new object store.
    ///
    /// * `name` - The name that the object store should be created with.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use indexeddb::IndexedDb;
    /// # use futures::executor::block_on;
    /// # block_on(async {
    /// let db = IndexedDb::open("test", 1, |_, db| {
    ///     db.create_object_store("test")
    ///         .expect("Couldn't create object store");
    /// }).await .expect("Failed to open indexed DB");
    /// # });
    pub fn create_object_store<'a>(
        &'a self,
        name: &str,
    ) -> Result<ObjectStoreDuringUpgrade<'a>, JsValue> {
        if self.store_exists(name) {
            return Err(format!("an object store called \"{}\" already exists", name).into());
        }

        let key_path: KeyPath = KeyPath::None;
        let key_path: JsValue = key_path.into();
        let mut parameters = web_sys::IdbObjectStoreParameters::new();

        parameters.key_path(Some(&key_path));
        parameters.auto_increment(false);

        let store = self
            .db
            .inner
            .create_object_store_with_optional_parameters(name, &parameters)?;

        Ok(ObjectStoreDuringUpgrade {
            inner: ObjectStore { inner: store },
            db: self,
        })
    }

    /// Is there already a store with the given name?
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the store that should be checked for existence.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use indexeddb::IndexedDb;
    /// # use futures::executor::block_on;
    /// # block_on(async {
    /// let db = IndexedDb::open("test", 1, |_, db| {
    ///     db.create_object_store("test")
    ///         .expect("Couldn't create object store");
    ///     assert!(db.store_exists("test"));
    /// }).await .expect("Failed to open indexed DB");
    /// # });
    /// ```
    pub fn store_exists(&self, name: &str) -> bool {
        self.db
            .object_store_names()
            .iter()
            .any(|store| store == name)
    }

    /// Deletes an object store
    pub(crate) fn delete_object_store(&self, name: &str) -> Result<(), JsValue> {
        self.db.inner.delete_object_store(name)?;
        Ok(())
    }
}

/// A handle to the opened database.
#[derive(Debug, Clone)]
pub struct IndexedDb {
    pub(crate) inner: Arc<web_sys::IdbDatabase>,
}

impl IndexedDb {
    /// Open a database with the given name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the database.
    ///
    /// * `version` - The current version of the database, if the database
    /// already existed but the given version is newer the `on_upgrade_needed`
    /// callback will be triggered. This needs to be a positive number bigger
    /// than zero.
    ///
    /// * `on_upgrade_needed` - Callback that will be called if the database
    /// needs to be upgraded, this includes the initial creation of the
    /// database.
    ///
    /// # Panics
    ///
    /// This method will panic if the given `version` is 0.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use indexeddb::IndexedDb;
    /// # use futures::executor::block_on;
    /// # block_on(async {
    /// let db = IndexedDb::open("test", 1, |_, db| {
    ///     db.create_object_store("test")
    ///         .expect("Couldn't create object store");
    /// }).await .expect("Failed to open indexed DB");
    ///
    /// assert_eq!(db.name(), "test");
    /// # });
    /// ```
    pub async fn open(
        name: &str,
        version: u32,
        on_upgrade_needed: impl Fn(u32, &DbDuringUpgrade) + 'static,
    ) -> Result<IndexedDb, JsValue> {
        if version == 0 {
            panic!("indexeddb version must be >= 1");
        }

        let request = factory().open_with_u32(name, version)?;
        let request = IdbOpenDbRequest::new(request, on_upgrade_needed);

        request.await
    }

    /// Get the name of this database.
    pub fn name(&self) -> String {
        self.inner.name()
    }

    /// The current version of the database.
    pub fn version(&self) -> u64 {
        self.inner.version() as u64
    }

    /// Get the names of the object stores in this database.
    pub fn object_store_names(&self) -> Vec<String> {
        to_collection!(self.inner.object_store_names() => Vec<String> : push)
    }

    /// Start a dababase transaction.
    ///
    /// All read/write operations in indexeddb need to happen using a
    /// transaction.
    ///
    /// This methods starts a new transaction which can be used to fetch an
    /// object store to read/write data out of it.
    ///
    /// Note that the transaction might autoclose if it doesn't have left
    /// anything to do. Awaiting on some other operation that doesn't use the
    /// transaction might result in a closed transaction.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use indexeddb::{IndexedDb, TransactionMode};
    /// # use futures::executor::block_on;
    /// # block_on(async {
    /// # let db = IndexedDb::open("test", 1, |_, db| {
    /// #   db.create_object_store("test").unwrap();
    /// # }).await .expect("Failed to open indexed DB");
    ///
    /// let transaction = db.transaction(TransactionMode::ReadWrite);
    /// let store = transaction.object_store("test").unwrap();
    ///
    /// // Do some reads/writes with the object store here, but do not await
    /// // some other future here since the transaction might autoclose.
    ///
    /// transaction.done().await;
    /// # });
    /// ```
    pub fn transaction(&self, mode: TransactionMode) -> Transaction {
        let inner = self
            .inner
            .transaction_with_str_sequence_and_mode(
                &self.inner.object_store_names().into(),
                mode.into(),
            )
            .unwrap();

        Transaction {
            inner,
            db: PhantomData,
        }
    }
}

#[cfg(test)]
mod test {
    use crate::IndexedDb;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    async fn open() {
        let db = IndexedDb::open("test", 1, |_old_version, _upgrader| ())
            .await
            .expect("Failed to open empty indexed db");

        assert_eq!(db.name(), "test");
        assert_eq!(db.version(), 1);
    }

    #[wasm_bindgen_test]
    async fn create_object_stores() {
        let db = IndexedDb::open("test2", 1, |_, upgrader| {
            let obj_store = upgrader.create_object_store("test").unwrap();

            drop(obj_store);
        })
        .await
        .expect("Failed to open indexed DB");

        assert!(!db.object_store_names().is_empty());
    }
}
