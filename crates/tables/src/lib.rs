pub mod cache;
pub mod clock;
pub mod fork;
pub mod google_sheets;
pub mod in_mem;
pub mod index;
pub mod search;

use async_trait::async_trait;
use std::{
    error::Error as StdError,
    sync::atomic::{AtomicU64, Ordering},
};

pub mod prelude {
    pub use crate::{TableClear, TableExtend, TableFetch, TableRead, TableUpdate, TableVersion};
}

#[async_trait]
pub trait TableFetch {
    type Entry<'a>: Send + Sync
    where
        Self: 'a;
    type Ok<'a>: IntoIterator<Item = Self::Entry<'a>> + Send
    where
        Self: 'a;
    type Error: StdError + Send;

    async fn fetch(&mut self) -> Result<Self::Ok<'_>, Self::Error>;

    async fn refresh(&mut self) -> Result<(), Self::Error> {
        let _ = self.fetch().await?;
        Ok(())
    }
}

#[async_trait]
pub trait TableRead {
    type Entry<'a>: Send + Sync
    where
        Self: 'a;
    type Ok<'a>: IntoIterator<Item = Self::Entry<'a>> + Send
    where
        Self: 'a;
    type Error: StdError + Send;

    fn read(&mut self) -> Result<Self::Ok<'_>, Self::Error>;
}

#[async_trait]
pub trait TableExtend<E: Send> {
    type Ok: Send;
    type Error: StdError + Send;

    async fn extend<'a, T>(&'a mut self, entries: T) -> Result<Self::Ok, Self::Error>
    where
        T: IntoIterator<Item = &'a E> + Clone + Send + Sync,
        E: 'a;

    async fn extend_owned<T>(&mut self, entries: T) -> Result<Self::Ok, Self::Error>
    where
        T: IntoIterator<Item = E> + Send,
        E: Sync,
    {
        let vec = entries.into_iter().collect::<Vec<_>>();
        self.extend(&vec).await
    }

    async fn extend_one(&mut self, entry: &E) -> Result<Self::Ok, Self::Error>
    where
        E: Sync,
    {
        self.extend([entry]).await
    }
}

#[async_trait]
pub trait TableUpdate<E: Send> {
    type Ok: Send;
    type Error: StdError + Send;

    async fn update<'a, T>(
        &'a mut self,
        from_row: usize,
        entries: T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: IntoIterator<Item = &'a E> + Clone + Send + Sync,
        E: 'a;

    async fn update_owned<T>(
        &mut self,
        from_row: usize,
        entries: T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: IntoIterator<Item = E> + Send,
        E: Sync,
    {
        let vec = entries.into_iter().collect::<Vec<_>>();
        self.update(from_row, &vec).await
    }

    async fn update_one(&mut self, row: usize, entry: &E) -> Result<Self::Ok, Self::Error>
    where
        E: Sync,
    {
        self.update(row, [entry]).await
    }
}

#[async_trait]
pub trait TableClear {
    type Error: StdError + Send;

    async fn clear(&mut self) -> Result<(), Self::Error>;
}

#[async_trait]
pub trait TableVersion {
    type Error: StdError + Send;

    async fn version(&mut self) -> Result<u64, Self::Error>;
}

static VERSION: AtomicU64 = AtomicU64::new(0);

pub(crate) fn next_version() -> u64 {
    VERSION.fetch_add(1, Ordering::Relaxed)
}
