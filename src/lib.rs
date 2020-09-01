#[macro_use]
mod macros;

mod db;
mod object_store;
mod request;
mod transaction;

use futures::{
    task::{Context, Poll},
    Future,
};
use std::{
    fmt,
    pin::Pin,
    sync::{Arc, Mutex},
};
use wasm_bindgen::{closure::Closure, JsCast, JsValue};

pub use crate::{db::*, object_store::*, transaction::*};

#[inline]
fn factory() -> web_sys::IdbFactory {
    web_sys::window().unwrap().indexed_db().unwrap().unwrap()
}

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

    let request = IdbOpenDbRequest::open(name, version)?;

    let request_copy = request.inner.clone();

    let onupgradeneeded = move |event: web_sys::IdbVersionChangeEvent| {
        let old_version = event.old_version() as u32;

        let result = match request_copy.result() {
            Ok(r) => r,
            Err(e) => panic!("Error before ugradeneeded: {:?}", e),
        };

        let db = DbDuringUpgrade::from_raw_unchecked(result, request_copy.clone());

        on_upgrade_needed(old_version, &db);
    };

    let onupgradeneeded =
        Closure::wrap(Box::new(onupgradeneeded) as Box<dyn FnMut(web_sys::IdbVersionChangeEvent)>);
    request.set_onupgradeneeded(Some(onupgradeneeded));

    Ok(request.await?)
}

/// Wraps the open db request. Private - the user interacts with the request using the function
/// passed to the `open` method.
struct IdbOpenDbRequest {
    // We need to move a ref for this into the upgradeneeded closure.
    inner: Arc<web_sys::IdbOpenDbRequest>,
    onsuccess: Mutex<Option<Closure<dyn FnMut()>>>,
    onerror: Mutex<Option<Closure<dyn FnMut()>>>,
    onupgradeneeded: Mutex<Option<Closure<dyn FnMut(web_sys::IdbVersionChangeEvent)>>>,
}

impl IdbOpenDbRequest {
    fn open(name: &str, version: u32) -> Result<IdbOpenDbRequest, JsValue> {
        // Can error because of origin rules.
        let inner = factory().open_with_u32(name, version)?;

        Ok(IdbOpenDbRequest {
            inner: Arc::new(inner),
            onsuccess: Mutex::new(None),
            onerror: Mutex::new(None),
            onupgradeneeded: Mutex::new(None),
        })
    }

    fn set_onsuccsess(&self, closure: Option<Closure<dyn FnMut()>>) {
        self.inner
            .set_onsuccess(closure.as_ref().map(|c| c.as_ref().unchecked_ref()));
        *self.onsuccess.lock().unwrap() = closure;
    }

    fn set_onerror(&self, closure: Option<Closure<dyn FnMut()>>) {
        self.inner
            .set_onerror(closure.as_ref().map(|c| c.as_ref().unchecked_ref()));
        *self.onerror.lock().unwrap() = closure;
    }

    fn set_onupgradeneeded(
        &self,
        closure: Option<Closure<dyn FnMut(web_sys::IdbVersionChangeEvent)>>,
    ) {
        self.inner
            .set_onupgradeneeded(closure.as_ref().map(|c| c.as_ref().unchecked_ref()));
        *self.onupgradeneeded.lock().unwrap() = closure;
    }
}

impl fmt::Debug for IdbOpenDbRequest {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "IdbOpenDbRequest")
    }
}

impl Future for IdbOpenDbRequest {
    type Output = Result<IndexedDb, JsValue>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        use web_sys::IdbRequestReadyState as ReadyState;

        match self.inner.ready_state() {
            ReadyState::Pending => {
                let waker = cx.waker().to_owned();

                // If we're not ready set up onsuccess and onerror callbacks to notify the
                // executor.
                let onsuccess =
                    Closure::wrap(Box::new(move || waker.clone().wake()) as Box<dyn FnMut()>);
                self.set_onsuccsess(Some(onsuccess));

                let waker = cx.waker().to_owned();

                let onerror =
                    Closure::wrap(Box::new(move || waker.clone().wake()) as Box<dyn FnMut()>);

                self.set_onerror(Some(onerror));

                Poll::Pending
            }
            ReadyState::Done => match self.inner.result() {
                Ok(val) => Poll::Ready(Ok(IndexedDb {
                    inner: Arc::new(val.unchecked_into()),
                })),
                Err(_) => match self.inner.error() {
                    Ok(Some(e)) => Poll::Ready(Err(e.into())),
                    Ok(None) => unreachable!("internal error polling open db request"),
                    Err(e) => Poll::Ready(Err(e)),
                },
            },
            _ => panic!("unexpected ready state"),
        }
    }
}
