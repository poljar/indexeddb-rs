use futures::{
    task::{Context, Poll},
    Future,
};
use serde::{Deserialize, Serialize};
use std::{
    marker::PhantomData,
    pin::Pin,
    sync::{Arc, Mutex},
};

use wasm_bindgen::{prelude::*, JsCast};

use crate::{db::DbDuringUpgrade, transaction::Transaction};

#[derive(Debug)]
pub struct ObjectStoreDuringUpgrade<'a> {
    pub(crate) inner: web_sys::IdbObjectStore,
    pub(crate) db: &'a DbDuringUpgrade,
}

impl<'a> ObjectStoreDuringUpgrade<'a> {
    /// The name of the object store.
    pub fn name(&self) -> String {
        self.inner.name()
    }

    /// The key path of the object store. No key path means keys are stored out-of-tree.
    pub fn key_path(&self) -> KeyPath {
        self.inner.key_path().unwrap().into()
    }

    /// Whether they primary key uses an auto-generated incrementing number as its value.
    pub fn auto_increment(&self) -> bool {
        self.inner.auto_increment()
    }

    /// Delete this object store.
    pub fn delete(self) -> Result<(), JsValue> {
        self.db.delete_object_store(&self.name())
    }
}

// impl<'a> Deref for ObjectStoreDuringUpgrade<'a> {
//     type Target = ObjectStore<'a>;

//     fn deref(&self) -> &Self::Target {
//         unsafe { mem::transmute(&self.inner) }
//     }
// }

#[derive(Debug)]
pub struct ObjectStore<'a> {
    pub(crate) inner: web_sys::IdbObjectStore,
    pub(crate) transaction: PhantomData<&'a Transaction<'a>>,
}

impl<'a> ObjectStore<'a> {
    /// The name of the object store.
    pub fn name(&self) -> String {
        self.inner.name()
    }

    /// Get the value with the given key.
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

    pub async fn add(&self, key: &impl Serialize, value: &impl Serialize) -> Result<(), JsValue> {
        let key = JsValue::from_serde(key).expect("Can't serialize key");
        let value = JsValue::from_serde(value).expect("Can't serialize value");

        let request = self.inner.add_with_key(&value, &key).unwrap();

        let request = IndexedDbRequest::new(request);
        let _ = request.await?;

        Ok(())
    }

    /// The key path of the object store. No key path means keys are stored out-of-tree.
    pub fn key_path(&self) -> KeyPath {
        self.inner.key_path().unwrap().into()
    }

    /// Whether they primary key uses an auto-generated incrementing number as its value.
    pub fn auto_increment(&self) -> bool {
        self.inner.auto_increment()
    }
}

/// The path to the key in an object store.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum KeyPath {
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

/// Wraps the open db request. Private - the user interacts with the request using the function
/// passed to the `open` method.
struct IndexedDbRequest {
    // We need to move a ref for this into the upgradeneeded closure.
    inner: Arc<web_sys::IdbRequest>,
    onsuccess: Mutex<Option<Closure<dyn FnMut()>>>,
    onerror: Mutex<Option<Closure<dyn FnMut()>>>,
}

impl IndexedDbRequest {
    fn new(request: web_sys::IdbRequest) -> Self {
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
