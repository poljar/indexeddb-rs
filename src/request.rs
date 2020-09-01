use futures::{
    task::{Context, Poll},
    Future,
};
use std::{
    pin::Pin,
    sync::{Arc, Mutex},
};

use wasm_bindgen::{closure::Closure, JsCast, JsValue};

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
