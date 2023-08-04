use async_trait::async_trait;
use log::info;
use pretty_type_name::pretty_type_name;
use std::{convert::Infallible, error::Error as StdError, fmt::Display};
use tokio::time::Instant;

use crate::{next_version, TableVersion};

use super::prelude::*;

macro_rules! try_cache {
    ($e:expr) => {
        $e.map_err(|e| Error::Cache(e))?
    };
}

macro_rules! try_origin {
    ($e:expr) => {
        $e.map_err(|e| Error::Origin(e))?
    };
}

#[derive(Debug)]
pub enum Error<O, C> {
    Origin(O),
    Cache(C),
}

impl<O: StdError, C: StdError> Display for Error<O, C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Origin(e) => write!(f, "origin error: {}", e),
            Error::Cache(e) => write!(f, "cache error: {}", e),
        }
    }
}

impl<O: StdError, C: StdError> StdError for Error<O, C> {}

pub struct Cache<O, C> {
    origin: O,
    cache: C,
    last_origin_version: u64,
}

impl<O, C> Cache<O, C> {
    pub fn new(origin: O, cache: C) -> Self {
        Self {
            origin,
            cache,
            last_origin_version: next_version(),
        }
    }

    pub fn mark_as_dirty(&mut self) {
        self.last_origin_version = next_version();
    }
}

#[async_trait]
impl<OErr, CErr, E, O, C> TableFetch for Cache<O, C>
where
    OErr: StdError + Send,
    CErr: StdError + Send,
    E: Send + Sync + 'static,
    for<'a> C: TableExtend<E, Error = CErr>
        + TableRead<Entry<'a> = &'a E, Error = CErr>
        + TableClear<Error = CErr>
        + Send
        + Sync
        + 'static,
    for<'a> O: TableFetch<Entry<'a> = E, Error = OErr>
        + TableVersion<Error = OErr>
        + Send
        + Sync
        + 'static,
{
    type Entry<'a> = <C as TableRead>::Entry<'a>;
    type Error = Error<OErr, CErr>;
    type Ok<'a> = <C as TableRead>::Ok<'a>;

    async fn fetch(&mut self) -> Result<Self::Ok<'_>, Self::Error> {
        self.refresh().await?;
        Ok(try_cache!(self.cache.read()))
    }

    async fn refresh(&mut self) -> Result<(), Self::Error> {
        let new_version = try_origin!(self.origin.version().await);
        if self.last_origin_version != new_version {
            info!(
                "Version of the origin ({}) changed from {} to {}",
                pretty_type_name::<O>(),
                self.last_origin_version,
                new_version
            );

            self.last_origin_version = new_version;

            info!("Fetch origin...");
            let now = Instant::now();
            let origin_entries = try_origin!(self.origin.fetch().await);
            info!("Origin fetched in {:?}", now.elapsed());

            info!("Rebuilding cache...");
            let now = Instant::now();
            try_cache!(self.cache.clear().await);
            try_cache!(self.cache.extend_owned(origin_entries).await);
            info!("Cache rebuilt in {:?}", now.elapsed());
        }

        Ok(())
    }
}

#[async_trait]
impl<Err, E, O, C> TableRead for Cache<O, C>
where
    Err: StdError + Send,
    E: Send + Sync + 'static,
    for<'a> C: TableUpdate<E, Error = Err>
        + TableRead<Entry<'a> = &'a E, Error = Err>
        + TableClear<Error = Err>
        + Send
        + Sync
        + 'static,
    for<'a> O: TableFetch<Entry<'a> = E> + Send + Sync + 'static,
{
    type Entry<'a> = <C as TableRead>::Entry<'a>;
    type Error = Error<O::Error, Err>;
    type Ok<'a> = <C as TableRead>::Ok<'a>;

    fn read(&mut self) -> Result<Self::Ok<'_>, Self::Error> {
        Ok(try_cache!(self.cache.read()))
    }
}

#[async_trait]
impl<Err, E, O, C> TableExtend<E> for Cache<O, C>
where
    Err: StdError + Send,
    E: Send + Sync + Clone,
    C: TableExtend<E, Error = Err> + TableClear<Error = Err> + Send + Sync + 'static,
    O: TableExtend<E> + Send + Sync,
{
    type Error = Error<O::Error, Err>;
    type Ok = O::Ok;

    async fn extend<'a, T>(&'a mut self, entries: T) -> Result<Self::Ok, Self::Error>
    where
        T: IntoIterator<Item = &'a E> + Send,
        E: 'a,
    {
        let entries = entries.into_iter().cloned().collect::<Vec<_>>();

        try_cache!(self.cache.extend(&entries).await);
        Ok(try_origin!(self.origin.extend(&entries).await))
    }
}

#[async_trait]
impl<E, O, C> TableUpdate<E> for Cache<O, C>
where
    E: Send + Sync + Clone,
    C: TableUpdate<E> + Send + Sync,
    O: TableUpdate<E> + Send + Sync,
{
    type Error = Error<O::Error, C::Error>;
    type Ok = O::Ok;

    async fn update<'a, T>(
        &'a mut self,
        from_row: usize,
        entries: T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: IntoIterator<Item = &'a E> + Send,
        E: 'a,
    {
        let entries = entries.into_iter().cloned().collect::<Vec<_>>();

        try_cache!(self.cache.update(from_row, &entries).await);
        Ok(try_origin!(self.origin.update(from_row, &entries).await))
    }
}

#[async_trait]
impl<O, C> TableVersion for Cache<O, C>
where
    O: Send + Sync,
    C: TableVersion + Send + Sync,
{
    type Error = Error<Infallible, C::Error>;

    async fn version(&mut self) -> Result<u64, Self::Error> {
        self.cache.version().await.map_err(|e| Error::Cache(e))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::in_mem::{InMemTable, ReadClone};

    #[tokio::test]
    async fn fetch() {
        let input = [0, 1, 2];
        let clock_origin: InMemTable<_, ReadClone> = input.into();
        let clock_cache: InMemTable<usize> = [].into();

        let mut table = Cache::new(clock_origin, clock_cache);

        let version = table.version().await.unwrap();
        let output: Vec<_> = table.fetch().await.unwrap().into_iter().cloned().collect();
        assert_eq!(output, input);

        assert_ne!(version, table.version().await.unwrap());
    }

    #[tokio::test]
    async fn insert() {
        let clock_origin: InMemTable<_, ReadClone> = [].into();
        let clock_cache: InMemTable<usize> = [].into();

        let mut table = Cache::new(clock_origin, clock_cache);

        let version = table.version().await.unwrap();
        let input = [0, 1, 2];

        table.extend(&input).await.unwrap();
        let output: Vec<_> = table.fetch().await.unwrap().into_iter().cloned().collect();

        assert_eq!(output, output);
        assert_ne!(version, table.version().await.unwrap());
    }

    #[tokio::test]
    async fn update() {
        let clock_origin: InMemTable<_, ReadClone> = [0, 1, 2].into();
        let clock_cache: InMemTable<usize> = [].into();

        let mut table = Cache::new(clock_origin, clock_cache);

        let version = table.version().await.unwrap();
        let input = [0, 1, 2];

        table.update(0, &input).await.unwrap();

        let output: Vec<_> = table.fetch().await.unwrap().into_iter().cloned().collect();
        assert_eq!(output, input);

        assert_ne!(version, table.version().await.unwrap());
    }
}
