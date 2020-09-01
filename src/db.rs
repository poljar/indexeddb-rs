use std::{marker::PhantomData, sync::Arc};
use wasm_bindgen::{prelude::*, JsCast};

use crate::{
    object_store::{KeyPath, ObjectStoreDuringUpgrade},
    transaction::{Transaction, TransactionMode},
};

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

    /// The name of the database.
    pub fn name(&self) -> String {
        self.db.name()
    }

    /// The current version.
    pub fn version(&self) -> u64 {
        self.db.version()
    }

    /// Creates a new object store (roughly equivalent to a table)
    pub fn create_object_store<'a>(
        &'a self,
        name: &str,
        key_path: impl Into<KeyPath>,
        auto_increment: bool,
    ) -> Result<ObjectStoreDuringUpgrade<'a>, JsValue> {
        if self.store_exists(name) {
            return Err(format!("an object store called \"{}\" already exists", name).into());
        }

        let key_path: KeyPath = key_path.into();
        let key_path: JsValue = key_path.into();
        let mut parameters = web_sys::IdbObjectStoreParameters::new();

        parameters.key_path(Some(&key_path));
        parameters.auto_increment(auto_increment);

        let store = self
            .db
            .inner
            .create_object_store_with_optional_parameters(name, &parameters)?;

        Ok(ObjectStoreDuringUpgrade {
            inner: store,
            db: self,
        })
    }

    /// Deletes an object store
    pub fn delete_object_store(&self, name: &str) -> Result<(), JsValue> {
        self.db.inner.delete_object_store(name)?;
        Ok(())
    }

    /// Is there already a store with the given name?
    fn store_exists(&self, name: &str) -> bool {
        self.db
            .object_store_names()
            .iter()
            .any(|store| store == name)
    }
}

/// A handle on the database
#[derive(Debug, Clone)]
pub struct IndexedDb {
    pub(crate) inner: Arc<web_sys::IdbDatabase>,
}

impl IndexedDb {
    pub async fn open(
        name: &str,
        version: u32,
        on_upgrade_needed: impl Fn(u32, &DbDuringUpgrade) + 'static,
    ) -> Result<IndexedDb, JsValue> {
        crate::open(name, version, on_upgrade_needed).await
    }

    /// The name of the database.
    pub fn name(&self) -> String {
        self.inner.name()
    }

    /// The current version.
    pub fn version(&self) -> u64 {
        self.inner.version() as u64
    }

    /// Get the names of the object stores in this database.
    pub fn object_store_names(&self) -> Vec<String> {
        to_collection!(self.inner.object_store_names() => Vec<String> : push)
    }

    /// Start a dababase transaction.
    ///
    /// All operations on data happen within a transaction, including read-only operations. I'm not
    /// sure yet whether beginning a transaction takes a snapshot or whether reads might give
    /// different answers.
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
    use crate::{IndexedDb, KeyPath};
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
            let obj_store = upgrader
                .create_object_store("test", KeyPath::None, false)
                .unwrap();
            assert_eq!(obj_store.key_path(), KeyPath::None);
            assert_eq!(obj_store.auto_increment(), false);

            drop(obj_store);

            let obj_store = upgrader
                .create_object_store("test2", KeyPath::Single("test".into()), true)
                .unwrap();
            assert_eq!(obj_store.key_path(), KeyPath::Single("test".into()));
            assert_eq!(obj_store.auto_increment(), true);

            drop(obj_store);

            let obj_store = upgrader
                .create_object_store(
                    "test3",
                    KeyPath::Multi(vec!["test".into(), "test2".into()]),
                    false,
                )
                .unwrap();

            assert_eq!(
                obj_store.key_path(),
                KeyPath::Multi(vec!["test".into(), "test2".into()])
            );
        })
        .await
        .expect("Failed to open indexed DB");

        assert!(!db.object_store_names().is_empty());
    }
}
