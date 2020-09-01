#[macro_use]
mod macros;

mod db;
mod object_store;
mod request;
mod transaction;

use wasm_bindgen::JsValue;

pub use crate::{
    db::{DbDuringUpgrade, IndexedDb},
    object_store::{KeyPath, ObjectStore, ObjectStoreDuringUpgrade},
    transaction::{Transaction, TransactionMode},
};

#[inline]
fn factory() -> web_sys::IdbFactory {
    web_sys::window().unwrap().indexed_db().unwrap().unwrap()
}

use crate::request::IdbOpenDbRequest;

/// Open a database.
///
/// # Panics
///
/// This function will panic if the new version is 0.
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

    Ok(request.await?)
}
