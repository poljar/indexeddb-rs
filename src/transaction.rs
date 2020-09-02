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

use crate::{IndexedDb, ObjectStore, TransactionObjectStore};

/// The mode the transaction should be opened in.
#[derive(Debug)]
pub enum TransactionMode {
    /// The transaction will be opened only for reading.
    Readonly,
    /// The transaction will be opened for reading and writing.
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

/// Struct representing an indexeddb transaction.
#[derive(Debug)]
pub struct Transaction<'a> {
    pub(crate) inner: IdbTransaction,
    pub(crate) db: PhantomData<&'a IndexedDb>,
}

impl<'a> Transaction<'a> {
    /// Get the object store with the given name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the object store that should be fetched.
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
    /// # });
    /// ```
    pub fn object_store(&self, name: &str) -> Result<TransactionObjectStore, JsValue> {
        let store = self.inner.object_store(name)?;

        Ok(TransactionObjectStore {
            inner: ObjectStore { inner: store },
            transaction: PhantomData,
        })
    }

    /// Wait for the transaction to be done.
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
    /// let key = "Hello".to_owned();
    /// let value = "world".to_owned();
    ///
    /// let transaction = db.transaction(TransactionMode::ReadWrite);
    /// let store = transaction.object_store("test").unwrap();
    ///
    /// store.add(&key, &value).await;
    /// transaction.done().await;
    /// # });
    /// ```
    pub async fn done(self) -> Result<(), JsValue> {
        let transaction = self.inner.clone();
        let transaction = TransactionFuture::new(transaction);

        transaction.await
    }

    /// Abort the transaction cancelling all the writes that were done using
    /// this transaction.
    pub async fn abort(self) -> Result<(), JsValue> {
        let transaction = self.inner.clone();
        let transaction = TransactionFuture::new(transaction);

        transaction.await
    }
}

/// State a transaction future can be in.
#[derive(Clone, Copy)]
enum TransactionState {
    Pending,
    Completed,
    Error,
    Aborted,
}

/// A future that allows waiting for a transaction to be done or aborted.
struct TransactionFuture {
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
    use crate::{IndexedDb, TransactionMode};
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    async fn await_transaction() {
        let db = IndexedDb::open("test2", 1, |_, db| {
            db.create_object_store("test").unwrap();
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
