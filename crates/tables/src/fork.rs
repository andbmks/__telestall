#[macro_export]
macro_rules! fork {
    ($fork_mod:ident : $fork_name:ident [$fork_e: ty], $or_name:ident : $or_ty:ty, $($sub_name:ident : $sub_ty:ty),+) => {
        #[allow(non_camel_case_types)]
        mod $fork_mod {
            use super::*;
            use $crate::prelude::*;

        #[derive(std::fmt::Debug)]
        pub enum Error {
            Fetch(ErrorFetch),
            Extend(ErrorExtend),
            Update(ErrorUpdate),
            Clear(ErrorClear),
        }

        impl std::fmt::Display for Error {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    Error::Fetch(e) => write!(f, "fetch error: {}", e),
                    Error::Extend(e) => write!(f, "extend error: {}", e),
                    Error::Update(e) => write!(f, "update error: {}", e),
                    Error::Clear(e) => write!(f, "clear error: {}", e),
                }
            }
        }

        impl std::error::Error for Error { }

        #[derive(std::fmt::Debug)]
        pub enum ErrorFetch {
            Origin(<$or_ty as TableFetch>::Error),
        }

        impl std::fmt::Display for ErrorFetch {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    ErrorFetch::Origin(e) => write!(f, "origin error: {}", e),
                }
            }
        }

        impl std::error::Error for ErrorFetch { }

        #[derive(std::fmt::Debug)]
        pub enum ErrorExtend {
            Origin(<$or_ty as TableExtend<$fork_e>>::Error),
            $($sub_name(<$sub_ty as TableExtend<$fork_e>>::Error)),+
        }

        impl std::fmt::Display for ErrorExtend {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    ErrorExtend::Origin(e) => write!(f, "origin error: {}", e),
                    $(ErrorExtend::$sub_name(e) => write!(f, "{} error: {}", stringify!($sub_name), e)),+
                }
            }
        }

        impl std::error::Error for ErrorExtend { }

        #[derive(std::fmt::Debug)]
        pub enum ErrorUpdate {
            Origin(<$or_ty as TableUpdate<$fork_e>>::Error),
            $($sub_name(<$sub_ty as TableUpdate<$fork_e>>::Error)),+
        }

        impl std::fmt::Display for ErrorUpdate {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    ErrorUpdate::Origin(e) => write!(f, "origin error: {}", e),
                    $(ErrorUpdate::$sub_name(e) => write!(f, "{} error: {}", stringify!($sub_name), e)),+
                }
            }
        }

        impl std::error::Error for ErrorUpdate { }

        #[derive(std::fmt::Debug)]
        pub enum ErrorClear {
            $($sub_name(<$sub_ty as TableClear>::Error)),+
        }

        impl std::fmt::Display for ErrorClear {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(ErrorClear::$sub_name(e) => write!(f, "{} error: {}", stringify!($sub_name), e)),+
                }
            }
        }

        impl std::error::Error for ErrorClear { }

        pub struct $fork_name {
            pub $or_name: $or_ty,
            $(pub $sub_name: $sub_ty),+
        }

        impl $fork_name {
            pub fn new($or_name: $or_ty, $($sub_name: $sub_ty),+) -> Self {
                Self {
                    $or_name,
                    $($sub_name),+
                }
            }
        }

        #[async_trait::async_trait]
        impl TableFetch for $fork_name {
            type Entry<'a> = <$or_ty as TableFetch>::Entry<'a>;
            type Ok<'a> = <$or_ty as TableFetch>::Ok<'a>;
            type Error = Error;

            async fn fetch(&mut self) -> Result<Self::Ok<'_>, Self::Error> {
                log::debug!("Fetching origin...");
                let now = tokio::time::Instant::now();
                let entries = self.$or_name.fetch().await.map_err(|e| Error::Fetch(ErrorFetch::Origin(e)))?;
                log::debug!("Fetched origin in {:?}", now.elapsed());

                log::debug!("Updating subscribers...");
                let now = tokio::time::Instant::now();
                $(self.$sub_name.clear().await.map_err(|e| Error::Clear(ErrorClear::$sub_name(e)))?;)+
                $(self.$sub_name.extend(entries.clone()).await.map_err(|e| Error::Extend(ErrorExtend::$sub_name(e)))?;)+
                log::debug!("Updated subscribers in {:?}", now.elapsed());

                Ok(entries)
            }
        }

        #[async_trait::async_trait]
        impl TableExtend<$fork_e> for $fork_name {
            type Ok = ();
            type Error = Error;

            async fn extend<'a, T>(&'a mut self, entries: T) -> Result<Self::Ok, Self::Error>
                where T: IntoIterator<Item=&'a $fork_e> + Clone + Sync + Send, $fork_e: 'a {

                log::debug!("Extending origin...");
                let now = tokio::time::Instant::now();
                self.$or_name.extend(entries.clone()).await.map_err(|e| Error::Extend(ErrorExtend::Origin(e)))?;
                log::debug!("Extended origin in {:?}", now.elapsed());

                log::debug!("Extending subscribers...");
                let now = tokio::time::Instant::now();
                $(self.$sub_name.extend(entries.clone()).await.map_err(|e| Error::Extend(ErrorExtend::$sub_name(e)))?;)+
                log::debug!("Extended subscribers in {:?}", now.elapsed());

                Ok(())
            }
        }

        #[async_trait::async_trait]
        impl TableUpdate<$fork_e> for $fork_name {
            type Ok = ();
            type Error = Error;

            async fn update<'a, T>(&'a mut self, from_row: usize, entries: T) -> Result<Self::Ok, Self::Error>
                where T: IntoIterator<Item=&'a $fork_e> + Clone + Send + Sync, $fork_e: 'a {

                let now = tokio::time::Instant::now();
                log::debug!("Updating origin...");
                self.$or_name.update(from_row, entries.clone()).await.map_err(|e| Error::Update(ErrorUpdate::Origin(e)))?;
                log::debug!("Updated origin in {:?}", now.elapsed());

                log::debug!("Updating subscribers...");
                let now = tokio::time::Instant::now();
                $(self.$sub_name.update(from_row, entries.clone()).await.map_err(|e| Error::Update(ErrorUpdate::$sub_name(e)))?;)+
                log::debug!("Updated subscribers in {:?}", now.elapsed());

                Ok(())
            }
        }

        }
        use $fork_mod::$fork_name;
    };
}
