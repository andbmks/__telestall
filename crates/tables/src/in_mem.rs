use std::{convert::Infallible, iter::FilterMap, slice::Iter};

use crate::{next_version, TableVersion};
use async_trait::async_trait;

use super::prelude::*;

pub struct ReadClone;
pub struct ReadRef;

#[derive(Default)]
pub struct InMemTable<E, M = ReadRef> {
    pub rows: Vec<Option<E>>,
    version: u64,
    _marker: std::marker::PhantomData<M>,
}

impl<M, E> InMemTable<E, M> {
    pub fn new(rows: Vec<E>) -> Self {
        Self {
            rows: rows.into_iter().map(Some).collect(),
            version: next_version(),
            _marker: std::marker::PhantomData,
        }
    }

    fn _extend<T: IntoIterator<Item = E>>(&mut self, entries: T) {
        self.rows.extend(entries.into_iter().map(Some));
        self.version = next_version();
    }

    fn _update<T: IntoIterator<Item = E>>(&mut self, from_row: usize, entries: T) {
        for (mut idx, entry) in entries.into_iter().enumerate() {
            idx += from_row;

            if idx >= self.rows.len() {
                self.rows.resize_with(idx + 1, || None);
            }

            self.rows[idx] = Some(entry);
        }
        self.version = next_version();
    }
}

#[async_trait]
impl<E: Send + Sync> TableFetch for InMemTable<E, ReadRef> {
    type Entry<'a> = &'a E where E: 'a;
    type Ok<'a> = FilterMap<Iter<'a, Option<E>>, fn(&'a Option<E>) -> Option<&'a E>> where E: 'a;
    type Error = Infallible;

    async fn fetch(&mut self) -> Result<Self::Ok<'_>, Self::Error> {
        self.read()
    }
}

#[async_trait]
impl<E: Send + Sync + Clone> TableFetch for InMemTable<E, ReadClone> {
    type Entry<'a> = E where E: 'a;
    type Ok<'a> = FilterMap<Iter<'a, Option<E>>, fn(&'a Option<E>) -> Option<E>> where E: 'a;
    type Error = Infallible;

    async fn fetch(&mut self) -> Result<Self::Ok<'_>, Self::Error> {
        self.read()
    }
}

#[async_trait]
impl<E: Send + Sync> TableRead for InMemTable<E, ReadRef> {
    type Entry<'a> = &'a E where E: 'a;
    type Ok<'a> = FilterMap<Iter<'a, Option<E>>, fn(&'a Option<E>) -> Option<&'a E>> where E: 'a;
    type Error = Infallible;

    fn read(&mut self) -> Result<Self::Ok<'_>, Self::Error> {
        Ok(self.rows.iter().filter_map(|v| v.as_ref()))
    }
}

#[async_trait]
impl<E: Send + Sync + Clone> TableRead for InMemTable<E, ReadClone> {
    type Entry<'a> = E where E: 'a;
    type Ok<'a> = FilterMap<Iter<'a, Option<E>>, fn(&'a Option<E>) -> Option<E>> where E: 'a;
    type Error = Infallible;

    fn read(&mut self) -> Result<Self::Ok<'_>, Self::Error> {
        Ok(self.rows.iter().filter_map(|v| v.clone()))
    }
}

#[async_trait]
impl<M: Send, E: Send + Clone> TableExtend<E> for InMemTable<E, M> {
    type Ok = ();
    type Error = Infallible;

    async fn extend<'a, T>(&'a mut self, entries: T) -> Result<Self::Ok, Self::Error>
    where
        T: IntoIterator<Item = &'a E> + Send,
    {
        self._extend(entries.into_iter().cloned());
        Ok(())
    }

    async fn extend_owned<T>(&mut self, entries: T) -> Result<Self::Ok, Self::Error>
    where
        T: IntoIterator<Item = E> + Send,
    {
        self._extend(entries);
        Ok(())
    }
}

#[async_trait]
impl<M: Send, E: Send + Clone> TableUpdate<E> for InMemTable<E, M> {
    type Ok = ();
    type Error = Infallible;

    async fn update<'a, T>(&'a mut self, from_row: usize, rows: T) -> Result<Self::Ok, Self::Error>
    where
        T: IntoIterator<Item = &'a E> + Send,
        E: 'a,
    {
        self._update(from_row, rows.into_iter().cloned());
        Ok(())
    }

    async fn update_owned<T>(
        &mut self,
        from_row: usize,
        entries: T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: IntoIterator<Item = E> + Send,
        E: Sync,
    {
        self._update(from_row, entries);
        Ok(())
    }
}

#[async_trait]
impl<M: Send, E: Send> TableClear for InMemTable<E, M> {
    type Error = Infallible;

    async fn clear(&mut self) -> Result<(), Self::Error> {
        self.rows.clear();
        self.version = 0;
        Ok(())
    }
}

#[async_trait]
impl<E: Send + Sync, M: Send + Sync> TableVersion for InMemTable<E, M> {
    type Error = Infallible;

    async fn version(&mut self) -> Result<u64, Self::Error> {
        Ok(self.version)
    }
}

impl<M, E: Clone, C: AsRef<[E]>> From<C> for InMemTable<E, M> {
    fn from(rows: C) -> Self {
        Self::new(rows.as_ref().to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fetch() {
        let mut table: InMemTable<u32> = [1, 2, 3].into();
        let version = table.version().await.unwrap();
        let rows: Vec<_> = table.fetch().await.unwrap().collect();

        assert_eq!(rows, vec![&1, &2, &3]);
        assert_eq!(version, table.version().await.unwrap());
    }

    #[tokio::test]
    async fn insert() {
        let mut table: InMemTable<usize> = [].into();
        let version = table.version().await.unwrap();
        let input = vec![0, 1, 2];

        table.extend(&input).await.unwrap();

        let output: Vec<_> = table.fetch().await.unwrap().into_iter().cloned().collect();

        assert_eq!(input, output);
        assert_ne!(version, table.version().await.unwrap());
    }

    #[tokio::test]
    async fn update() {
        let mut table: InMemTable<usize> = [].into();
        let version = table.version().await.unwrap();
        let input: Vec<_> = vec![0, 1, 2];

        table.update(0, &input).await.unwrap();

        let output: Vec<_> = table.fetch().await.unwrap().into_iter().cloned().collect();

        assert_eq!(input, output);
        assert_ne!(version, table.version().await.unwrap());
    }
}
