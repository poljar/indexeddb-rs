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

use crate::db::{DbDuringUpgrade, IndexedDb};

pub(crate) struct IndexedDbRequest {
    inner: Arc<web_sys::IdbRequest>,
    onsuccess: Mutex<Option<Closure<dyn FnMut()>>>,
    onerror: Mutex<Option<Closure<dyn FnMut()>>>,
}

impl IndexedDbRequest {
    pub(crate) fn new(request: web_sys::IdbRequest) -> Self {
        Self {
            inner: Arc::new(request),
            onsuccess: Mutex::new(None),
            onerror: Mutex::new(None),
        }
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
}

impl Future for IndexedDbRequest {
    type Output = Result<JsValue, JsValue>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        use web_sys::IdbRequestReadyState as ReadyState;

        match self.inner.ready_state() {
            ReadyState::Pending => {
                let waker = cx.waker().to_owned();

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
                Ok(val) => Poll::Ready(Ok(val)),

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

/// Wraps the open db request. Private - the user interacts with the request using the function
/// passed to the `open` method.
pub(crate) struct IdbOpenDbRequest {
    // We need to move a ref for this into the upgradeneeded closure.
    pub(crate) inner: Arc<web_sys::IdbOpenDbRequest>,
    onsuccess: Mutex<Option<Closure<dyn FnMut()>>>,
    onerror: Mutex<Option<Closure<dyn FnMut()>>>,
    onupgradeneeded: Mutex<Option<Closure<dyn FnMut(web_sys::IdbVersionChangeEvent)>>>,
}

impl IdbOpenDbRequest {
    pub(crate) fn new(
        request: web_sys::IdbOpenDbRequest,
        upgrade_callback: impl Fn(u32, &DbDuringUpgrade) + 'static,
    ) -> Self {
        let request = Arc::new(request);
        let request_copy = request.clone();

        let request = IdbOpenDbRequest {
            inner: request,
            onsuccess: Mutex::new(None),
            onerror: Mutex::new(None),
            onupgradeneeded: Mutex::new(None),
        };

        let onupgradeneeded = move |event: web_sys::IdbVersionChangeEvent| {
            let old_version = event.old_version() as u32;

            let result = match request_copy.result() {
                Ok(r) => r,
                Err(e) => panic!("Error before ugradeneeded: {:?}", e),
            };

            let db = DbDuringUpgrade::from_raw_unchecked(result, request_copy.clone());
            upgrade_callback(old_version, &db);
        };

        let onupgradeneeded = Closure::wrap(
            Box::new(onupgradeneeded) as Box<dyn FnMut(web_sys::IdbVersionChangeEvent)>
        );
        request.set_onupgradeneeded(Some(onupgradeneeded));

        request
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

    pub(crate) fn set_onupgradeneeded(
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
