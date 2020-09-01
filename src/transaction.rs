use std::{
    marker::PhantomData,
    pin::Pin,
    sync::{Arc, Mutex},
};

use futures::{
    task::{Context, Poll},
    Future,
};

use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{IdbTransaction, IdbTransactionMode};

use crate::{IndexedDb, ObjectStore};

pub enum TransactionMode {
    Readonly,
    ReadWrite,
}

impl Into<IdbTransactionMode> for TransactionMode {
    fn into(self) -> IdbTransactionMode {
        match self {
            TransactionMode::Readonly => IdbTransactionMode::Readonly,
            TransactionMode::ReadWrite => IdbTransactionMode::Readwrite,
        }
    }
}

pub struct Transaction<'a> {
    pub(crate) inner: IdbTransaction,
    pub(crate) db: PhantomData<&'a IndexedDb>,
}

impl<'a> Transaction<'a> {
    pub fn object_store(&self, name: &str) -> Result<ObjectStore, JsValue> {
        let store = self
            .inner
            .object_store(name)?;

        Ok(ObjectStore {
            inner: store,
            transaction: PhantomData,
        })
    }

    pub async fn done(self) -> Result<(), JsValue> {
        let transaction = self.inner.clone();
        let transaction = TransactionFuture::new(transaction);

        transaction.await
    }

    pub async fn abort(self) -> Result<(), JsValue> {
        let transaction = self.inner.clone();
        let transaction = TransactionFuture::new(transaction);

        transaction.await
    }
}

#[derive(Clone, Copy)]
pub enum TransactionState {
    Pending,
    Completed,
    Error,
    Aborted,
}

pub struct TransactionFuture {
    inner: IdbTransaction,
    state: Arc<Mutex<TransactionState>>,
    on_completed: Mutex<Option<Closure<dyn FnMut()>>>,
    on_error: Mutex<Option<Closure<dyn FnMut()>>>,
    on_abort: Mutex<Option<Closure<dyn FnMut()>>>,
}

impl TransactionFuture {
    fn new(transaction: IdbTransaction) -> Self {
        Self {
            inner: transaction,
            state: Arc::new(Mutex::new(TransactionState::Pending)),
            on_completed: Mutex::new(None),
            on_error: Mutex::new(None),
            on_abort: Mutex::new(None),
        }
    }

    fn state(&self) -> TransactionState {
        *self.state.lock().unwrap()
    }

    fn set_on_complete(&self, closure: Option<Closure<dyn FnMut()>>) {
        self.inner
            .set_oncomplete(closure.as_ref().map(|c| c.as_ref().unchecked_ref()));
        *self.on_completed.lock().unwrap() = closure;
    }

    fn set_on_error(&self, closure: Option<Closure<dyn FnMut()>>) {
        self.inner
            .set_onerror(closure.as_ref().map(|c| c.as_ref().unchecked_ref()));
        *self.on_error.lock().unwrap() = closure;
    }

    fn set_on_abort(&self, closure: Option<Closure<dyn FnMut()>>) {
        self.inner
            .set_onabort(closure.as_ref().map(|c| c.as_ref().unchecked_ref()));
        *self.on_abort.lock().unwrap() = closure;
    }
}

impl Future for TransactionFuture {
    type Output = Result<(), JsValue>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.state() {
            TransactionState::Pending => {
                let waker = cx.waker().to_owned();
                let state = self.state.clone();

                let on_complete = Closure::wrap(Box::new(move || {
                    *state.lock().unwrap() = TransactionState::Completed;
                    waker.clone().wake()
                }) as Box<dyn FnMut()>);
                self.set_on_complete(Some(on_complete));

                let waker = cx.waker().to_owned();
                let state = self.state.clone();

                let on_error = Closure::wrap(Box::new(move || {
                    *state.lock().unwrap() = TransactionState::Error;
                    waker.clone().wake()
                }) as Box<dyn FnMut()>);

                self.set_on_error(Some(on_error));

                let waker = cx.waker().to_owned();
                let state = self.state.clone();

                let on_abort = Closure::wrap(Box::new(move || {
                    *state.lock().unwrap() = TransactionState::Aborted;
                    waker.clone().wake()
                }) as Box<dyn FnMut()>);

                self.set_on_abort(Some(on_abort));

                Poll::Pending
            }
            TransactionState::Completed => Poll::Ready(Ok(())),
            TransactionState::Error => Poll::Ready(Err(self.inner.error().into())),
            TransactionState::Aborted => Poll::Ready(Err(JsValue::undefined())),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{IndexedDb, KeyPath, TransactionMode};
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    async fn await_transaction() {
        let db = IndexedDb::open("test2", 1, |_, upgrader| {
            upgrader
                .create_object_store("test", KeyPath::None, false)
                .unwrap();
        })
        .await
        .expect("Failed to open indexed DB");

        let transaction = db.transaction(TransactionMode::ReadWrite);

        let store = transaction.object_store("test").unwrap();
        let key = "Hello".to_owned();

        store
            .add(&key, &"world".to_owned())
            .await
            .expect("Can't write to the store");
        transaction
            .done()
            .await
            .expect("Can't await end of transaction");

        let transaction = db.transaction(TransactionMode::Readonly);
        let store = transaction.object_store("test").unwrap();

        let value: String = store
            .get(&key)
            .await
            .expect("Can't get string out of store")
            .unwrap();
        assert_eq!(value, "world");
    }
}
