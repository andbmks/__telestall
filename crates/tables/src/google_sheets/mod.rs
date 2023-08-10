pub mod range;
pub mod serde_impl;

use async_trait::async_trait;
use google_sheets4::{
    api::{self as sheets4, RowData},
    hyper::client::HttpConnector,
    hyper_rustls::HttpsConnector,
    Error as SheetsError, FieldMask, Sheets,
};
use lazy_static::lazy_static;
use log::info;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{error::Error as StdError, fmt::Display, str::FromStr, sync::Arc};
use tokio::time::Instant;

use self::range::SheetRange;
use self::serde_impl::{Error as SerdeError, RowDeserializer, RowSerializer};
use crate::{next_version, prelude::*, TableVersion};

lazy_static! {
    static ref RANGE_LASTROW_REGEX: Regex = Regex::new(r"^.*![A-Z]+\d+:[A-Z]+(\d+)$").unwrap();
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    InvalidResponse,
    InvalidMeta,
    Sheets(SheetsError),
    Serde(SerdeError),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InvalidResponse => f.write_str("invalid response"),
            Error::InvalidMeta => f.write_str("invalid meta"),
            Error::Sheets(e) => f.write_str(&format!("sheets error: {}", e)),
            Error::Serde(e) => f.write_str(&format!("serde error: {}", e)),
        }
    }
}

impl StdError for Error {}

#[derive(Default, Deserialize)]
struct SheetArgsInput {
    pub id: i32,
    pub data_range: SheetRange,
    pub format_range: Option<SheetRange>,
    pub meta_range: Option<SheetRange>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(from = "SheetArgsInput")]
pub struct SheetArgs {
    pub id: i32,
    pub data_range: SheetRange,
    pub format_range: SheetRange,
    pub meta_range: Option<SheetRange>,
}

impl From<SheetArgsInput> for SheetArgs {
    fn from(value: SheetArgsInput) -> Self {
        Self {
            id: value.id,
            format_range: value.format_range.unwrap_or(value.data_range.clone()),
            data_range: value.data_range,
            meta_range: value.meta_range,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct MetaEntry {
    pub hash: String,
}

#[derive(Clone)]
pub struct Sheet<E> {
    hub: Arc<Sheets<HttpsConnector<HttpConnector>>>,
    spreadsheet_id: String,
    args: SheetArgs,
    version: u64,
    version_hash: String,
    _marker: std::marker::PhantomData<E>,
}

impl<E> Sheet<E> {
    pub fn new(
        hub: Arc<Sheets<HttpsConnector<HttpConnector>>>,
        spreadsheet_id: String,
        args: SheetArgs,
    ) -> Self {
        Self {
            hub,
            spreadsheet_id,
            args,
            version: 0,
            version_hash: "".to_owned(),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn remake<T>(self) -> Sheet<T> {
        Sheet {
            hub: self.hub,
            spreadsheet_id: self.spreadsheet_id,
            args: self.args,
            version: self.version,
            version_hash: self.version_hash,
            _marker: std::marker::PhantomData,
        }
    }

    async fn update_cells(&mut self, requests: Vec<sheets4::Request>) -> Result<()> {
        let request = sheets4::BatchUpdateSpreadsheetRequest {
            include_spreadsheet_in_response: Some(false),
            requests: Some(requests),
            response_include_grid_data: Some(false),
            response_ranges: None,
        };

        self.hub
            .spreadsheets()
            .batch_update(request, &self.spreadsheet_id)
            .doit()
            .await
            .map_err(|e| Error::Sheets(e))?;

        self.fetch_version().await?;

        Ok(())
    }

    async fn fetch_last_available_row(&self) -> Result<usize> {
        let response = self
            .hub
            .spreadsheets()
            .values_append(
                sheets4::ValueRange::default(),
                &self.spreadsheet_id,
                &self.args.data_range.with_inf_end().to_string(),
            )
            .value_input_option("USER_ENTERED")
            .doit()
            .await
            .map_err(|e| Error::Sheets(e))?
            .1;

        if let Some(range) = response.table_range {
            return RANGE_LASTROW_REGEX
                .captures(&range)
                .and_then(|captures| {
                    captures
                        .get(1)
                        .and_then(|m| m.as_str().parse::<usize>().ok())
                })
                .ok_or(Error::InvalidResponse);
        } else {
            return Ok(self.args.data_range.r_start);
        }
    }

    async fn fetch_version(&mut self) -> Result<()> {
        let range = match self.args.meta_range {
            Some(ref range) => range.clone(),
            None => {
                self.version = next_version();
                return Ok(());
            }
        };

        let mut values = self
            .hub
            .spreadsheets()
            .values_get(&self.spreadsheet_id, &range.to_string())
            .doit()
            .await
            .map_err(|e| Error::Sheets(e))?
            .1
            .values;

        let meta_hash = match values {
            Some(ref mut row) => {
                let mut deserializer = RowDeserializer::new(&mut row[0]);
                let meta =
                    MetaEntry::deserialize(&mut deserializer).map_err(|e| Error::Serde(e))?;
                meta.hash
            }
            None => "".to_owned(),
        };

        if meta_hash != self.version_hash {
            self.version = next_version();
            self.version_hash = meta_hash;
        }

        Ok(())
    }
}

impl<E: Serialize> Sheet<E> {
    async fn extend_impl(mut self, entries: Vec<E>) -> Result<()> {
        let row_from = self.fetch_last_available_row().await?;

        let row_data = entries
            .into_iter()
            .map(|entry| {
                let mut serializer = RowSerializer::default();
                entry
                    .serialize(&mut serializer)
                    .map_err(|e| Error::Serde(e))?;
                Ok(serializer.into())
            })
            .collect::<Result<Vec<_>>>()?;

        let row_to = row_from + row_data.len();

        let data_range = self
            .args
            .data_range
            .with_rows(row_from, row_to)
            .as_grid_range(self.args.id);

        let format_range = self
            .args
            .format_range
            .with_rows(row_from, row_to)
            .as_grid_range(self.args.id);

        let insert_dimension = sheets4::Request {
            insert_dimension: Some(sheets4::InsertDimensionRequest {
                inherit_from_before: Some(false),
                range: Some(sheets4::DimensionRange {
                    dimension: Some("ROWS".to_owned()),
                    end_index: Some(1 + row_to as i32),
                    sheet_id: Some(self.args.id),
                    start_index: Some(1 + row_from as i32),
                }),
            }),
            ..Default::default()
        };

        let source_range = self
            .args
            .format_range
            .with_rows(row_from - 1, row_from)
            .as_grid_range(self.args.id);

        let paste_normal = sheets4::Request {
            copy_paste: Some(sheets4::CopyPasteRequest {
                source: Some(source_range.clone()),
                destination: Some(format_range.clone()),
                paste_orientation: Some("NORMAL".to_owned()),
                paste_type: Some("PASTE_NORMAL".to_owned()),
            }),
            ..Default::default()
        };

        let paste_data_validation = sheets4::Request {
            copy_paste: Some(sheets4::CopyPasteRequest {
                source: Some(source_range.clone()),
                destination: Some(format_range.clone()),
                paste_orientation: Some("NORMAL".to_owned()),
                paste_type: Some("PASTE_DATA_VALIDATION".to_owned()),
            }),
            ..Default::default()
        };

        let update_cells = sheets4::Request {
            update_cells: Some(sheets4::UpdateCellsRequest {
                fields: Some(FieldMask::from_str("userEnteredValue").unwrap()),
                range: Some(data_range),
                rows: Some(row_data),
                start: None,
            }),
            ..Default::default()
        };

        self.update_cells(vec![
            insert_dimension,
            paste_normal,
            update_cells,
            paste_data_validation,
        ])
        .await?;

        Ok(())
    }

    async fn update_impl(mut self, mut from_row: usize, entries: Vec<E>) -> Result<()> {
        from_row += self.args.data_range.r_start;

        let rows = entries
            .into_iter()
            .map(|entry| {
                let mut serializer = RowSerializer::default();
                entry
                    .serialize(&mut serializer)
                    .map_err(|e| Error::Serde(e))?;
                Ok(serializer.into())
            })
            .collect::<Result<Vec<RowData>>>()?;

        let request = sheets4::Request {
            update_cells: Some(sheets4::UpdateCellsRequest {
                fields: Some(FieldMask::from_str("userEnteredValue").unwrap()),
                range: Some(
                    self.args
                        .data_range
                        .with_rows(from_row, from_row + rows.len())
                        .as_grid_range(self.args.id),
                ),
                rows: Some(rows),
                start: None,
            }),
            ..Default::default()
        };

        self.update_cells(vec![request]).await
    }
}

#[async_trait]
impl<'de, E: Deserialize<'de> + Send + Sync> TableFetch for Sheet<E> {
    type Entry<'a> = E where E: 'a;
    type Error = Error;
    type Ok<'a> = Vec<E> where Self: 'a, E: 'a;

    async fn fetch(&mut self) -> Result<Self::Ok<'_>> {
        info!("Fetching sheet data...");
        let now = Instant::now();
        let range = self
            .hub
            .spreadsheets()
            .values_get(
                &self.spreadsheet_id,
                &self.args.data_range.with_inf_end().to_string(),
            )
            .doit()
            .await
            .map_err(|e| Error::Sheets(e))?
            .1;

        info!("Sheet data fetched in {:?}", now.elapsed());

        info!("Deserializing sheet data...");
        let now = Instant::now();
        if let Some(values) = range.values {
            let result = values
                .into_iter()
                .map(|mut data| {
                    let mut deserializer = RowDeserializer::new(&mut data);
                    match E::deserialize(&mut deserializer) {
                        Ok(entry) => Some(entry),
                        Err(_) => None,
                    }
                })
                .filter_map(|e| e)
                .collect::<Vec<_>>();

            info!("Sheet data deserialized in {:?}", now.elapsed());
            Ok(result)
        } else {
            info!("Deserializing was skipped");
            Ok(vec![])
        }
    }
}

#[async_trait]
impl<E: Serialize + Send + Sync + Clone + 'static> TableExtend<E> for Sheet<E> {
    type Error = Error;
    type Ok = ();

    async fn extend<'a, T>(&'a mut self, entries: T) -> Result<()>
    where
        T: IntoIterator<Item = &'a E> + Send,
        E: 'a,
    {
        let entries = entries.into_iter().cloned().collect();
        tokio::task::spawn(self.clone().extend_impl(entries));

        Ok(())
    }
}

#[async_trait]
impl<E: Serialize + Send + Sync + Clone + 'static> TableUpdate<E> for Sheet<E> {
    type Error = Error;
    type Ok = ();

    async fn update<'a, T>(&'a mut self, from_row: usize, entries: T) -> Result<()>
    where
        T: IntoIterator<Item = &'a E> + Send,
        E: 'a,
    {
        let entries = entries.into_iter().cloned().collect();
        tokio::task::spawn(self.clone().update_impl(from_row, entries));

        Ok(())
    }
}

#[async_trait]
impl<E: Send + Sync> TableVersion for Sheet<E> {
    type Error = Error;

    async fn version(&mut self) -> Result<u64> {
        self.fetch_version().await?;
        Ok(self.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use google_sheets4::{
        hyper::{self, client::HttpConnector},
        hyper_rustls::{self, HttpsConnector},
        oauth2,
        oauth2::ServiceAccountAuthenticator,
        Sheets,
    };
    use serde::{Deserialize, Serialize};

    const TEST_SPREADSHEET_ID: &'static str = "1CA7P-kYPQInSIiT-Jp0W-JTwqMWmB7tDA8trfOwBCU0";

    #[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
    struct TestEntry {
        string: String,
        int: f64,
        boolean: bool,
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug, Default, Copy, Clone)]
    struct EmptyEntry {
        a: (),
        b: (),
        c: (),
    }

    #[tokio::test]
    async fn fetch() {
        let hub = Arc::new(build_hub().await);
        let mut sheet = Sheet::new(
            hub.clone(),
            TEST_SPREADSHEET_ID.to_owned(),
            SheetArgsInput {
                id: 1068945262,
                data_range: SheetRange::from_str("Read!A2:C").unwrap(),
                meta_range: Some(SheetRange::from_str("Meta!B2").unwrap()),
                ..Default::default()
            }
            .into(),
        );
        _fetch(&mut sheet).await;
    }

    async fn _fetch(sheet: &mut Sheet<TestEntry>) {
        let old_version = sheet.version().await.unwrap();
        let entries: Vec<TestEntry> = sheet.fetch().await.unwrap();

        let new_version = sheet.version().await.unwrap();
        assert_eq!(old_version, new_version);
        assert_entries(&entries);
    }

    #[tokio::test]
    async fn update() {
        let hub = Arc::new(build_hub().await);
        let sheet = Sheet::new(
            hub.clone(),
            TEST_SPREADSHEET_ID.to_owned(),
            SheetArgsInput {
                id: 697802184,
                data_range: SheetRange::from_str("Update!A2:C").unwrap(),
                meta_range: Some(SheetRange::from_str("Meta!B3").unwrap()),
                ..Default::default()
            }
            .into(),
        );
        _update(sheet).await;
    }

    async fn _update(mut sheet: Sheet<TestEntry>) {
        let entries = [
            TestEntry {
                string: "A".to_owned(),
                int: 1.0,
                boolean: true,
            },
            TestEntry {
                string: "B".to_owned(),
                int: 2.0,
                boolean: false,
            },
            TestEntry {
                string: "C".to_owned(),
                int: 3.0,
                boolean: true,
            },
        ];

        let old_version = sheet.version().await.unwrap();
        sheet.update(0, &entries).await.unwrap();
        let new_version = sheet.version().await.unwrap();

        assert_ne!(old_version, new_version);
        _fetch(&mut sheet).await;

        let trash = [
            TestEntry {
                string: "C".to_owned(),
                int: 1.0,
                boolean: true,
            },
            TestEntry {
                string: "C".to_owned(),
                int: 1.0,
                boolean: true,
            },
            TestEntry {
                string: "C".to_owned(),
                int: 1.0,
                boolean: true,
            },
        ];
        sheet.update(0, &trash).await.unwrap();
    }

    #[tokio::test]
    async fn extend() {
        let hub = Arc::new(build_hub().await);
        let sheet = Sheet::new(
            hub.clone(),
            TEST_SPREADSHEET_ID.to_owned(),
            SheetArgsInput {
                id: 3715267,
                data_range: SheetRange::from_str("Extend!A2:C").unwrap(),
                meta_range: Some(SheetRange::from_str("Meta!B4").unwrap()),
                ..Default::default()
            }
            .into(),
        );
        _extend(sheet).await;
    }

    async fn _extend(mut sheet: Sheet<TestEntry>) {
        let entries = [
            TestEntry {
                string: "A".to_owned(),
                int: 1.0,
                boolean: true,
            },
            TestEntry {
                string: "B".to_owned(),
                int: 2.0,
                boolean: false,
            },
            TestEntry {
                string: "C".to_owned(),
                int: 3.0,
                boolean: true,
            },
        ];

        let old_version = sheet.version().await.unwrap();
        sheet.extend(&entries).await.unwrap();
        let new_version = sheet.version().await.unwrap();

        assert_ne!(old_version, new_version);
        _fetch(&mut sheet).await;

        sheet
            .remake::<EmptyEntry>()
            .update(0, &[EmptyEntry::default(); 3])
            .await
            .unwrap();
    }

    fn assert_entries(rows: &[TestEntry]) {
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows[0],
            TestEntry {
                string: "A".to_owned(),
                int: 1.0,
                boolean: true
            }
        );
        assert_eq!(
            rows[1],
            TestEntry {
                string: "B".to_owned(),
                int: 2.0,
                boolean: false
            }
        );
        assert_eq!(
            rows[2],
            TestEntry {
                string: "C".to_owned(),
                int: 3.0,
                boolean: true
            }
        );
    }

    async fn build_hub() -> Sheets<HttpsConnector<HttpConnector>> {
        let creds = oauth2::read_service_account_key("./../../credentials.json")
            .await
            .expect("Can't read credential.");

        let auth = ServiceAccountAuthenticator::builder(creds)
            .build()
            .await
            .expect("There was an error, trying to build connection with authenticator");

        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .build();

        Sheets::new(hyper::Client::builder().build(connector), auth)
    }
}
