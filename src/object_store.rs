use std::{marker::PhantomData, ops::Deref};

use serde::{Deserialize, Serialize};
use wasm_bindgen::{prelude::*, JsCast};

use crate::{db::DbDuringUpgrade, request::IndexedDbRequest, transaction::Transaction};

/// An object store that was created during an upgrade.
///
/// Object stores can only be created and deleted during database upgrades.
#[derive(Debug)]
pub struct ObjectStoreDuringUpgrade<'a> {
    pub(crate) inner: ObjectStore,
    pub(crate) db: &'a DbDuringUpgrade,
}

impl<'a> ObjectStoreDuringUpgrade<'a> {
    /// Delete this object store.
    pub fn delete(self) -> Result<(), JsValue> {
        self.db.delete_object_store(&self.name())
    }
}

impl<'a> Deref for ObjectStoreDuringUpgrade<'a> {
    type Target = ObjectStore;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// An object store that is bound to a transaction.
#[derive(Debug)]
pub struct TransactionObjectStore<'a> {
    pub(crate) inner: ObjectStore,
    pub(crate) transaction: PhantomData<&'a Transaction<'a>>,
}

impl<'a> Deref for TransactionObjectStore<'a> {
    type Target = ObjectStore;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Base object store that gathers all the common object store functionality.
#[derive(Debug)]
pub struct ObjectStore {
    pub(crate) inner: web_sys::IdbObjectStore,
}

impl<'a> ObjectStore {
    /// The name of the object store.
    pub fn name(&self) -> String {
        self.inner.name()
    }

    /// Get the value with the given key.
    ///
    /// # Arguments
    ///
    /// * `key` - The key that should be used to find the associated value in
    /// the store.
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
    /// let transaction = db.transaction(TransactionMode::Readonly);
    /// let store = transaction.object_store("test").unwrap();
    ///
    /// let key = "Hello".to_owned();
    ///
    /// let value: String = store
    ///     .get(&key)
    ///     .await
    ///     .expect("Store error while fetching value")
    ///     .unwrap();
    /// # });
    /// ```
    pub async fn get<V: for<'b> Deserialize<'b>>(
        &self,
        key: &impl Serialize,
    ) -> Result<Option<V>, JsValue> {
        let key = JsValue::from_serde(&key).expect("Can't serialize key");
        let request = self.inner.get(&key)?;

        let request = IndexedDbRequest::new(request);

        let object = request.await?;

        if object.is_undefined() || object.is_null() {
            Ok(None)
        } else {
            Ok(object.into_serde().expect("Can't deserialize value"))
        }
    }

    /// Store the given value under the given key in the object store.
    ///
    /// # Arguments
    ///
    /// * `key` - The key that should be used to save the associated value in
    /// the store.
    ///
    /// * `value` - The value that should saved in the store.
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
    /// let transaction = db.transaction(TransactionMode::ReadWrite);
    /// let store = transaction.object_store("test").unwrap();
    ///
    /// let key = "Hello".to_owned();
    /// let value = "world".to_owned();
    ///
    /// store.add(&key, &value).await.unwrap();
    /// transaction.done().await;
    ///
    /// # });
    /// ```
    pub async fn add(&self, key: &impl Serialize, value: &impl Serialize) -> Result<(), JsValue> {
        let key = JsValue::from_serde(key).expect("Can't serialize key");
        let value = JsValue::from_serde(value).expect("Can't serialize value");

        let request = self.inner.add_with_key(&value, &key).unwrap();

        let request = IndexedDbRequest::new(request);
        let _ = request.await?;

        Ok(())
    }

    /// The key path of the object store. No key path means keys are stored
    /// out-of-tree.
    #[allow(dead_code)]
    fn key_path(&self) -> KeyPath {
        self.inner.key_path().unwrap().into()
    }
}

/// The path to the key in an object store.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum KeyPath {
    /// Keys are stored *out-of-tree*.
    None,
    /// The path to the single key.
    Single(String),
    // This complains when I use it in the browser TODO investigate.
    // /// The paths to all the parts of the key.
    Multi(Vec<String>),
}

impl From<KeyPath> for JsValue {
    fn from(key_path: KeyPath) -> JsValue {
        match key_path {
            KeyPath::None => JsValue::NULL,
            KeyPath::Single(path) => JsValue::from(path),
            KeyPath::Multi(paths) => from_collection!(paths).into(),
        }
    }
}

impl From<JsValue> for KeyPath {
    fn from(val: JsValue) -> KeyPath {
        if val.is_null() || val.is_undefined() {
            KeyPath::None
        } else if let Some(s) = val.as_string() {
            KeyPath::Single(s)
        } else {
            let arr = match val.dyn_into::<js_sys::Array>() {
                Ok(v) => v,
                Err(e) => panic!("expected array of strings, found {:?}", e),
            };

            let mut out = vec![];

            for el in arr.values().into_iter() {
                let el = el.unwrap();
                if let Some(val) = el.as_string() {
                    out.push(val);
                } else {
                    panic!("Expected string, found {:?}", el);
                }
            }

            KeyPath::Multi(out)
        }
    }
}

impl From<Vec<String>> for KeyPath {
    fn from(inner: Vec<String>) -> KeyPath {
        KeyPath::Multi(inner)
    }
}

impl<S> From<&[S]> for KeyPath
where
    S: AsRef<str>,
{
    fn from(inner: &[S]) -> KeyPath {
        KeyPath::Multi(inner.iter().map(|s| s.as_ref().to_owned()).collect())
    }
}

impl From<String> for KeyPath {
    fn from(inner: String) -> KeyPath {
        KeyPath::Single(inner)
    }
}

impl<'a> From<&'a str> for KeyPath {
    fn from(inner: &'a str) -> KeyPath {
        KeyPath::Single(inner.to_owned())
    }
}

impl From<()> for KeyPath {
    fn from((): ()) -> KeyPath {
        KeyPath::None
    }
}
