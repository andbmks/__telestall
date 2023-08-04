use crate::{next_version, prelude::*};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use log::info;
use pretty_type_name::pretty_type_name;

pub struct Clock<I> {
    inner: I,
    ttl: Duration,
    cached_version: u64,
    last_cache_update: DateTime<Utc>,
}

impl<I> Clock<I> {
    pub fn new(inner: I, ttl: Duration) -> Self {
        Self {
            inner,
            ttl,
            cached_version: next_version(),
            last_cache_update: Utc::now(),
        }
    }
}

#[async_trait]
impl<I: TableVersion + Send> TableVersion for Clock<I> {
    type Error = I::Error;

    async fn version(&mut self) -> Result<u64, Self::Error> {
        let now = Utc::now();
        if now - self.last_cache_update > self.ttl {
            info!(
                "Version ttl of the {} expired ({})",
                pretty_type_name::<I>(),
                self.ttl
            );

            self.cached_version = self.inner.version().await?;
            self.last_cache_update = now;
        }

        Ok(self.cached_version)
    }
}

#[async_trait]
impl<I: TableFetch + Send + 'static> TableFetch for Clock<I> {
    type Entry<'a> = I::Entry<'a>;
    type Ok<'a> = I::Ok<'a>;
    type Error = I::Error;

    async fn fetch(&mut self) -> Result<Self::Ok<'_>, Self::Error> {
        self.inner.fetch().await
    }

    async fn refresh(&mut self) -> Result<(), Self::Error> {
        self.inner.refresh().await
    }
}

#[async_trait]
impl<I: TableRead + Send + 'static> TableRead for Clock<I> {
    type Entry<'a> = I::Entry<'a>;
    type Ok<'a> = I::Ok<'a>;
    type Error = I::Error;

    fn read(&mut self) -> Result<Self::Ok<'_>, Self::Error> {
        self.inner.read()
    }
}

#[async_trait]
impl<I: TableUpdate<E> + Send + 'static, E: Send> TableUpdate<E> for Clock<I> {
    type Ok = I::Ok;
    type Error = I::Error;

    async fn update<'a, T>(
        &'a mut self,
        from_row: usize,
        entries: T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: IntoIterator<Item = &'a E> + Clone + Send + Sync,
        E: 'a,
    {
        self.inner.update(from_row, entries).await
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
        self.inner.update_owned(from_row, entries).await
    }

    async fn update_one(&mut self, row: usize, entry: &E) -> Result<Self::Ok, Self::Error>
    where
        E: Sync,
    {
        self.inner.update_one(row, entry).await
    }
}

#[async_trait]
impl<I: TableExtend<E> + Send + 'static, E: Send> TableExtend<E> for Clock<I> {
    type Ok = I::Ok;
    type Error = I::Error;

    async fn extend<'a, T>(&'a mut self, entries: T) -> Result<Self::Ok, Self::Error>
    where
        T: IntoIterator<Item = &'a E> + Clone + Send + Sync,
        E: 'a,
    {
        self.inner.extend(entries).await
    }

    async fn extend_owned<T>(&mut self, entries: T) -> Result<Self::Ok, Self::Error>
    where
        T: IntoIterator<Item = E> + Send,
        E: Sync,
    {
        self.inner.extend_owned(entries).await
    }

    async fn extend_one(&mut self, entry: &E) -> Result<Self::Ok, Self::Error>
    where
        E: Sync,
    {
        self.inner.extend_one(entry).await
    }
}

#[async_trait]
impl<I: TableClear + Send + 'static> TableClear for Clock<I> {
    type Error = I::Error;

    async fn clear(&mut self) -> Result<(), Self::Error> {
        self.inner.clear().await
    }
}
