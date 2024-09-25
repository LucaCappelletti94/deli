use std::marker::PhantomData;

use idb::request::{DeleteStoreRequest, OpenKeyCursorStoreRequest};

use crate::{Error, Model, Transaction};

/// Key cursor on an object store or index
#[derive(Debug)]
pub struct KeyCursor<'t, M>
where
    M: Model,
{
    cursor: idb::KeyCursor,
    _transaction: &'t Transaction,
    _generics_model: PhantomData<M>,
}

impl<'t, M> KeyCursor<'t, M>
where
    M: Model,
{
    /// Creates a new instance of cursor
    pub(crate) fn new(transaction: &'t Transaction, cursor: idb::KeyCursor) -> Self {
        Self {
            cursor,
            _transaction: transaction,
            _generics_model: Default::default(),
        }
    }

    /// Returns the key at current cursor position
    pub fn get_key(&self) -> Result<Option<M::Key>, Error> {
        let js_value = self.cursor.primary_key()?;
        serde_wasm_bindgen::from_value(js_value).map_err(Into::into)
    }

    /// Advances the cursor
    pub fn advance(&mut self, count: u32) -> Result<OpenKeyCursorStoreRequest, Error> {
        self.cursor.advance(count).map_err(Into::into)
    }

    /// Deletes the entry at current cursor position
    pub fn delete(&self) -> Result<DeleteStoreRequest, Error> {
        self.cursor.delete().map_err(Into::into)
    }
}
