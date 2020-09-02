//! # `indexeddb`
//!
//! Higher level bindings for the IndexedDB client side storage database that
//! most modern Web browsers support.
//!
//! This crate wraps the low level web-sys bindings for IndexedDB converting the
//! API to a Rust Future based API. The crate will not work outside of a
//! browser.
//!
//! # Example
//!
//! ```no_run
//! # use futures::executor::block_on;
//! use indexeddb::{IndexedDb, TransactionMode};
//!
//! # block_on(async {
//! let db = IndexedDb::open("test", 1, |_, db| {
//!    db.create_object_store("test").unwrap();
//! }).await .expect("Failed to open indexed DB");
//!
//! let transaction = db.transaction(TransactionMode::ReadWrite);
//! let store = transaction.object_store("test").unwrap();
//!
//! let key = "Hello".to_owned();
//! let value = "world".to_owned();
//!
//! store.add(&key, &value).await;
//! transaction.done().await;
//!
//! let transaction = db.transaction(TransactionMode::Readonly);
//! let store = transaction.object_store("test").unwrap();
//!
//! let loaded_value: String = store
//!     .get(&key)
//!     .await
//!     .expect("Store error while fetching value")
//!     .unwrap();
//!
//! assert_eq!(value, loaded_value);
//! # });
//! ```
#![deny(
    missing_debug_implementations,
    dead_code,
    missing_docs,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications
)]

#[macro_use]
mod macros;

mod db;
mod object_store;
mod request;
mod transaction;

pub use crate::{
    db::{DbDuringUpgrade, IndexedDb},
    object_store::{ObjectStore, ObjectStoreDuringUpgrade, TransactionObjectStore},
    transaction::{Transaction, TransactionMode},
};
