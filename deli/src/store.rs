use std::{borrow::Borrow, convert::TryInto, marker::PhantomData};

use idb::{ObjectStore, Query};
use serde::{de::DeserializeOwned, Serialize};
use serde_wasm_bindgen::Serializer;

use crate::{
    Cursor, Direction, Error, Index, KeyCursor, KeyRange, Model, SpecificKeyRange, Transaction,
};

/// An object store in indexed db (with add, update and index function)
#[derive(Debug)]
pub struct NonGenericStore<'t, 'a, M>
where
    M: Model,
{
    store: &'a ObjectStore,
    transaction: &'t Transaction,
    _generics: PhantomData<M>,
}

impl<'t, 'a, M> NonGenericStore<'t, 'a, M>
where
    M: Model,
{
    /// Creates a new instance of store
    pub(crate) fn new(transaction: &'t Transaction, store: &'a ObjectStore) -> Self {
        Self {
            store,
            transaction,
            _generics: Default::default(),
        }
    }

    /// Adds a value to the store returning its key
    pub async fn add<V>(&self, value: &V) -> Result<M::Key, Error>
    where
        V: Serialize,
    {
        let value = value.serialize(&Serializer::json_compatible())?;
        let js_key = self.store.add(&value, None)?.await?;
        serde_wasm_bindgen::from_value(js_key).map_err(Into::into)
    }

    /// Updates a value in the store returning its key
    pub async fn update<V>(&self, value: &V) -> Result<M::Key, Error>
    where
        V: Serialize,
    {
        let value = value.serialize(&Serializer::json_compatible())?;
        let js_key = self.store.put(&value, None)?.await?;
        serde_wasm_bindgen::from_value(js_key).map_err(Into::into)
    }

    /// Returns object store index with given name and data type
    pub fn index<T>(&self, name: &str) -> Result<Index<'t, M, T>, Error>
    where
        T: Serialize + DeserializeOwned,
    {
        let index = self.store.index(name)?;
        Ok(Index::new(self.transaction, index))
    }
}

/// An object store in indexed db
#[derive(Debug)]
pub struct Store<'t, M>
where
    M: Model,
{
    store: ObjectStore,
    transaction: &'t Transaction,
    _generics: PhantomData<M>,
}

impl<'t, M> Store<'t, M>
where
    M: Model,
{
    /// Creates a new instance of store
    pub(crate) fn new(transaction: &'t Transaction, store: ObjectStore) -> Self {
        Self {
            store,
            transaction,
            _generics: Default::default(),
        }
    }

    /// Returns an add and update enabled store
    #[doc(hidden)]
    pub fn non_generic_store(&self) -> NonGenericStore<'_, '_, M> {
        NonGenericStore::new(self.transaction, &self.store)
    }

    /// Counts all the values from store with given query and limit
    pub async fn count<'a, K>(
        &self,
        query: impl Into<KeyRange<'a, M, M::Key, K>>,
    ) -> Result<u32, Error>
    where
        M::Key: Borrow<K>,
        K: Serialize + ?Sized + 'a,
    {
        self.store
            .count(query.into().try_into()?)?
            .await
            .map_err(Into::into)
    }

    /// Gets value for specifier key
    pub async fn get<K>(&self, key: &K) -> Result<Option<M>, Error>
    where
        M::Key: Borrow<K>,
        K: Serialize + ?Sized,
    {
        let key = key.serialize(&Serializer::json_compatible())?;
        let js_value = self.store.get(key)?.await?;

        js_value
            .map(|js_value| serde_wasm_bindgen::from_value(js_value).map_err(Into::into))
            .transpose()
    }

    /// Gets all the values from store with given query and limit
    pub async fn get_all<'a, K>(
        &self,
        query: impl Into<KeyRange<'a, M, M::Key, K>>,
        limit: Option<u32>,
    ) -> Result<Vec<M>, Error>
    where
        M::Key: Borrow<K>,
        K: Serialize + ?Sized + 'a,
    {
        let js_values = self.store.get_all(query.into().try_into()?, limit)?.await?;

        js_values
            .into_iter()
            .map(|js_value| serde_wasm_bindgen::from_value(js_value).map_err(Into::into))
            .collect()
    }

    /// Gets all the keys from store with given query and limit
    pub async fn get_all_keys<'a, K>(
        &self,
        query: impl Into<KeyRange<'a, M, M::Key, K>>,
        limit: Option<u32>,
    ) -> Result<Vec<M::Key>, Error>
    where
        M::Key: Borrow<K>,
        K: Serialize + ?Sized + 'a,
    {
        let js_keys = self
            .store
            .get_all_keys(query.into().try_into()?, limit)?
            .await?;

        js_keys
            .into_iter()
            .map(|js_value| serde_wasm_bindgen::from_value(js_value).map_err(Into::into))
            .collect()
    }

    /// Scans the store for values
    pub async fn scan<'a, K>(
        &self,
        query: impl Into<KeyRange<'a, M, M::Key, K>>,
        direction: Option<Direction>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<M>, Error>
    where
        M::Key: Borrow<K>,
        K: Serialize + ?Sized + 'a,
    {
        let Some(mut cursor) = self.cursor(query, direction).await? else {
            return Ok(Vec::new());
        };

        if let Some(offset) = offset {
            if let Some(new_cursor) = cursor.advance(offset).await? {
                *cursor = new_cursor;
            } else {
                return Ok(Vec::new());
            }
        }

        match limit {
            Some(limit) => {
                let mut values = Vec::new();

                for _ in 0..limit {
                    match cursor.get_value()? {
                        Some(value) => {
                            values.push(value);
                            if let Some(new_cursor) = cursor.advance(1).await? {
                                *cursor = new_cursor;
                            } else {
                                break;
                            }
                        }
                        None => break,
                    }
                }

                Ok(values)
            }
            None => {
                let mut values = Vec::new();

                while let Some(value) = cursor.get_value()? {
                    values.push(value);
                    if let Some(new_cursor) = cursor.advance(1).await? {
                        *cursor = new_cursor;
                    } else {
                        break;
                    }
                }

                Ok(values)
            }
        }
    }

    /// Scans the store for keys
    pub async fn scan_keys<'a, K>(
        &self,
        query: impl Into<KeyRange<'a, M, M::Key, K>>,
        direction: Option<Direction>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<M::Key>, Error>
    where
        M::Key: Borrow<K>,
        K: Serialize + ?Sized + 'a,
    {
        let Some(mut cursor) = self.key_cursor(query, direction).await? else {
            return Ok(Vec::new());
        };

        if let Some(offset) = offset {
            if let Some(new_cursor) = cursor.advance(offset).await? {
                *cursor = new_cursor;
            } else {
                return Ok(Vec::new());
            }
        }

        match limit {
            Some(limit) => {
                let mut keys = Vec::new();

                for _ in 0..limit {
                    match cursor.get_key()? {
                        Some(value) => {
                            keys.push(value);
                            if let Some(new_cursor) = cursor.advance(1).await? {
                                *cursor = new_cursor;
                            } else {
                                break;
                            }
                        }
                        None => break,
                    }
                }

                Ok(keys)
            }
            None => {
                let mut keys = Vec::new();

                while let Some(value) = cursor.get_key()? {
                    keys.push(value);
                    if let Some(new_cursor) = cursor.advance(1).await? {
                        *cursor = new_cursor;
                    } else {
                        break;
                    }
                }

                Ok(keys)
            }
        }
    }

    /// Returns a cursor on object store
    pub async fn cursor<'a, K>(
        &self,
        query: impl Into<KeyRange<'a, M, M::Key, K>>,
        direction: Option<Direction>,
    ) -> Result<Option<M::Cursor<'t>>, Error>
    where
        M::Key: Borrow<K>,
        K: Serialize + ?Sized + 'a,
    {
        let cursor = self
            .store
            .open_cursor(query.into().try_into()?, direction)?
            .await?;

        Ok(cursor.map(|c| Cursor::new(self.transaction, c).into()))
    }

    /// Returns a key cursor on object store
    pub async fn key_cursor<'a, K>(
        &self,
        query: impl Into<KeyRange<'a, M, M::Key, K>>,
        direction: Option<Direction>,
    ) -> Result<Option<M::KeyCursor<'t>>, Error>
    where
        M::Key: Borrow<K>,
        K: Serialize + ?Sized + 'a,
    {
        let cursor = self
            .store
            .open_key_cursor(query.into().try_into()?, direction)?
            .await?;

        Ok(cursor.map(|c| KeyCursor::new(self.transaction, c).into()))
    }

    /// Deletes value with specified key
    pub async fn delete<'a, K>(
        &self,
        query: impl Into<SpecificKeyRange<'a, M, M::Key, K>>,
    ) -> Result<(), Error>
    where
        M::Key: Borrow<K>,
        K: Serialize + ?Sized + 'a,
    {
        let query: Query = query.into().try_into()?;
        self.store.delete(query)?.await.map_err(Into::into)
    }
}
