use std::marker::PhantomData;

use web_sys::{IdbTransaction, IdbTransactionMode};

use crate::Db;

pub enum TransactionMode {}

impl Into<IdbTransactionMode> for TransactionMode {
    fn into(self) -> IdbTransactionMode {
        todo!()
    }
}

pub struct Transaction<'a> {
    pub(crate) inner: IdbTransaction,
    pub(crate) db: PhantomData<&'a Db>,
}

pub struct TransactionDuringUpgrade<'a> {
    pub(crate) inner: IdbTransaction,
    pub(crate) db: &'a Db,
}
