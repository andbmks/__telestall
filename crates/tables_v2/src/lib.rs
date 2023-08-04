use std::collections::HashMap;
use std::sync::Arc;

use google_sheets4::{self as s4, hyper::client::HttpConnector, hyper_rustls::HttpsConnector};
use tokio::{sync::RwLock, task::JoinSet};

pub struct LocalSpreadsheet {
    name: String,
    sheets: HashMap<String, LocalSheet>,
}

impl LocalSpreadsheet {
    pub fn get_or_create(&mut self, sheet_name: &str) -> &mut LocalSheet {
        self.sheets
            .entry(sheet_name.to_string())
            .or_insert_with(|| LocalSheet::empty())
    }

    pub fn get(&self, sheet_name: &str) -> Option<&LocalSheet> {
        self.sheets.get(sheet_name)
    }

    pub fn get_mut(&self, sheet_name: &str) -> Option<&mut LocalSheet> {
        self.sheets.get_mut(sheet_name)
    }
}

pub struct LocalSheet {
    width: usize,
    height: usize,
    values: Vec<Value>,
}

impl LocalSheet {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            values: vec![Value::None; width * height],
        }
    }

    pub fn empty() -> Self {
        Self::new(0, 0)
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn fit(&mut self, new_width: usize) {
        let mut new_values = vec![Value::None; new_width * self.height];

        for y in 0..self.height {
            for x in 0..self.width {
                let rev_x = self.width - x - 1;
                new_values[y * new_width + x] = self.values[y * self.width + rev_x].clone();
            }
        }

        self.values = new_values;
    }

    pub fn inflate(&mut self, new_width: usize) {
        if new_width < self.width {
            return;
        }

        let mut new_values = vec![Value::None; new_width * self.height];

        for y in 0..self.height {
            for x in 0..self.width {
                new_values[y * new_width + x] = self.values[y * self.width + x].clone();
            }
        }

        self.values = new_values;
    }

    pub fn prepare(&mut self, height: usize) {
        let requested_len = self.width * height;

        if requested_len > self.values.len() {
            self.values.resize(requested_len, Value::None);
            self.height = height;
        }
    }

    pub fn get(&self, x: usize, y: usize) -> &Value {
        &self.values[y * self.width + x]
    }

    pub fn get_column(&self, x: usize) -> impl Iterator<Item = &Value> {
        self.values.iter().step_by(self.width).skip(x)
    }

    pub fn set(&mut self, x: usize, y: usize, value: Value) {
        self.values[y * self.width + x] = value;
    }

    pub fn push(&mut self, row: Vec<Value>) {
        if row.len() != self.width {
            panic!("self.width != {}", row.len());
        }

        self.values.extend(row);
        self.height += 1;
    }

    pub fn append_column(&mut self, column: Vec<Value>) {
        if column.len() != self.height {
            panic!("self.height != {}", column.len());
        }

        self.inflate(self.width + 1);

        for y in 0..self.height {
            self.values[y * self.width + self.width - 1] = column[y].clone();
        }

        self.width += 1;
    }

    pub fn replace(&mut self, x: usize, y: usize, row: Vec<Value>) {
        if row.len() != (self.width + x) {
            panic!("self.width != {} + {x}", row.len());
        }

        self.values
            .splice((y * self.width + x)..y * self.width, row.into_iter());
    }
}

#[derive(Debug, Clone, Default)]
pub enum Value {
    #[default]
    None,
    Number(f64),
    String(String),
    Bool(bool),
}

impl From<s4::api::ExtendedValue> for Value {
    fn from(value: s4::api::ExtendedValue) -> Self {
        match value {
            s4::api::ExtendedValue {
                number_value: Some(number),
                ..
            } => Self::Number(number),
            s4::api::ExtendedValue {
                bool_value: Some(bool),
                ..
            } => Self::Bool(bool),
            s4::api::ExtendedValue {
                string_value: Some(string),
                ..
            } => Self::String(string),
            _ => Self::None,
        }
    }
}

pub enum FetchRequest {
    Select(SelectRequest),
}

impl FetchRequest {
    pub(crate) async fn execute(
        self,
        connection: Arc<Connection>,
        properties: Arc<SpreadsheetProperties>,
    ) -> Result<LocalSheet, RequestError> {
        match self {
            Self::Select(select_req) => select_req.execute(connection, properties).await,
        }
    }
}

pub struct SelectRequest {
    pub sub_requests: Vec<SelectSubRequest>,
    pub operators: Vec<SelectOperator>,
}

impl SelectRequest {
    pub(crate) async fn execute(
        self,
        connection: Arc<Connection>,
        properties: Arc<SpreadsheetProperties>,
        local_spreadsheet: Arc<RwLock<LocalSpreadsheet>>,
    ) -> Result<LocalSheet, RequestError> {
        let mut data_filters = vec![];

        // Collect sub requests with their sheet properties
        let sub_requests = self
            .sub_requests
            .into_iter()
            .map(|sub_request| {
                let sheet_name = &sub_request.sheet_name;
                let sheet_properties = properties
                    .sheet(&sheet_name)
                    .ok_or(RequestError::SheetDoesntExist(sheet_name.clone()))?;

                Ok((sub_request, sheet_properties))
            })
            .collect::<Result<Vec<_>, _>>()?;

        for (sub_request, sheet_properties) in sub_requests.iter() {
            sub_request.fill_s4_data_filters(sheet_properties, &mut data_filters)?;
        }

        let s4_request = s4::api::GetSpreadsheetByDataFilterRequest {
            data_filters: Some(data_filters),
            ..Default::default()
        };

        // Fetch spreadsheet
        let spreadsheet = connection
            .spreadsheets()
            .get_by_data_filter(s4_request, &properties.id)
            .param("fields", "sheets.data.rowData.values.effectiveValue")
            .doit()
            .await
            .map_err(|e| RequestError::Sheets4Error(e.into()))?
            .1;

        // Extract sheets from spreadsheet
        let sheets = spreadsheet
            .sheets
            .ok_or(RequestError::NoSheetsInSpreadsheet)?
            .into_iter()
            .filter_map(|sheet| {
                let name = sheet
                    .properties
                    .as_ref()
                    .map(|props| props.title.as_ref())
                    .flatten()
                    .unwrap()
                    .clone();

                sheet.data.map(|data| (name, data))
            });

        let mut local_spreadsheet = local_spreadsheet.write().await;

        // Fill local spreadsheet with fetched values
        // todo: can be split in tasks
        for (sheet_name, grids) in sheets {
            let local_sheet = local_spreadsheet.get_or_create(&sheet_name);

            for grid in grids {
                let s4::api::GridData { start_row: Some(start_row), start_column: Some(start_col), row_data: Some(rows), ..} = grid else {
                    return Err(RequestError::InvalidSheet(sheet_name));
                };

                if rows.len() == 0 {
                    continue;
                }

                let start_row = start_row as usize;
                let start_col = start_col as usize;

                let width = start_col + rows[0].values.as_ref().unwrap().len();
                local_sheet.fit(width);

                let height = start_row + rows.len();
                local_sheet.prepare(height);

                for (idx, row) in rows.into_iter().enumerate() {
                    if let Some(row) = row.values {
                        let values: Vec<Value> = row
                            .into_iter()
                            .map(|cell| cell.effective_value.map_or(Value::None, |v| v.into()))
                            .collect();

                        local_sheet.replace(start_col, idx + start_row, values)
                    }
                }
            }
        }

        let mut tables = vec![];

        for (sub_request, sheet_properties) in sub_requests {
            sub_request.make_table(
                sheet_properties,
                local_spreadsheet.get_mut(&sheet_properties.name).unwrap(), // todo: handle unwrap
            )?;
        }

        // todo: apply operators
        Ok(tables.pop().unwrap())
    }
}

pub struct SelectSubRequest {
    pub sheet_name: String,
    pub columns: Vec<String>,
    pub predicates: Vec<Predicate>,
    pub operators: Vec<PredicateOperator>,
}

impl SelectSubRequest {
    pub(crate) fn fill_s4_data_filters(
        &self,
        properties: &SheetProperties,
        data_filters: &mut Vec<s4::api::DataFilter>,
    ) -> Result<(), RequestError> {
        if self.columns.is_empty() {
            return Ok(());
        }

        let mut column_indexes = self.map_columns_to_indexes(properties)?;

        // Sort indexes so that we can group them into ranges
        column_indexes.sort();

        let mut ranges: Vec<s4::api::GridRange> = vec![];

        let new_range = |col_idx| s4::api::GridRange {
            sheet_id: Some(properties.id()),
            start_column_index: Some(col_idx as i32),
            end_column_index: Some(1 + col_idx as i32),
            start_row_index: Some(1), // 1 because first row is header
            end_row_index: None,      // unbounded
        };

        // Group column indexes into ranges
        column_indexes[1..]
            .iter()
            .fold(new_range(column_indexes[0]), |mut range, col_idx| {
                let end_col_idx = range.end_column_index.as_mut().unwrap();
                if *col_idx == *end_col_idx as usize {
                    *end_col_idx += 1;
                    range
                } else {
                    ranges.push(range);
                    new_range(*col_idx)
                }
            });

        // Fill the vector
        data_filters.extend(ranges.into_iter().map(|range| s4::api::DataFilter {
            grid_range: Some(range),
            ..Default::default()
        }));

        Ok(())
    }

    pub(crate) fn make_table(
        &self,
        properties: &SheetProperties,
        local_sheet: &mut LocalSheet,
    ) -> Result<LocalSheet, RequestError> {
        let mut output_sheet = LocalSheet::new(self.columns.len(), 0);

        let column_indexes = self.map_columns_to_indexes(properties)?;

        for col_idx in column_indexes {
            let values = local_sheet.get_column(col_idx).cloned().collect();
            output_sheet.append_column(values);
        }

        Ok(output_sheet)
    }

    fn map_columns_to_indexes(
        &self,
        properties: &SheetProperties,
    ) -> Result<Vec<usize>, RequestError> {
        // todo: check for duplicates
        self.columns
            .iter()
            .map(|col| {
                properties
                    .column_idx(col)
                    .cloned()
                    .ok_or(RequestError::ColumnDoesntExist(col.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()
    }
}

pub enum SelectOperator {
    Union,
}

pub struct SubRequestInsert {
    pub table: String,
    pub columns: Vec<String>,
    pub values: Vec<Value>,
}

pub struct SubRequestUpdate {
    pub table: String,
    pub columns: Vec<String>,
    pub values: Vec<Value>,
    pub predicates: Vec<Predicate>,
    pub operators: Vec<PredicateOperator>,
}

pub enum Predicate {
    Equality(String, Value),
}

pub enum PredicateOperator {
    And,
    Or,
    Not,
}

pub enum RequestError {
    SheetDoesntExist(String),
    ColumnDoesntExist(String),
    NoSheetsInSpreadsheet,
    NoDataInSheet(String),
    InvalidSheet(String),
    Sheets4Error(s4::Error),
}

pub struct Spreadsheet {
    con: Arc<Connection>,
    properties: Arc<SpreadsheetProperties>,
}

impl Spreadsheet {
    pub async fn fetch_raw(
        &self,
        f: impl FnOnce(&mut SpreadsheetFetchCursor),
    ) -> Result<LocalSheet, SpreadsheetError> {
        let mut cursor = SpreadsheetFetchCursor::new(self.con.clone(), self.properties.clone());
        f(&mut cursor);

        unimplemented!()
    }
}

pub enum SpreadsheetError {
    RequestError(RequestError),
}

pub struct SpreadsheetFetchCursor {
    con: Arc<Connection>,
    properties: Arc<SpreadsheetProperties>,
    requests: Vec<FetchRequest>,
}

impl SpreadsheetFetchCursor {
    pub(crate) fn new(con: Arc<Connection>, properties: Arc<SpreadsheetProperties>) -> Self {
        Self {
            con,
            properties,
            requests: vec![],
        }
    }

    pub fn execute<Err>(
        &mut self,
        request: impl TryInto<FetchRequest, Error = Err>,
    ) -> Result<&mut Self, SpreadsheetCursorError>
    where
        Err: std::error::Error + 'static,
    {
        let request = request
            .try_into()
            .map_err(|e| SpreadsheetCursorError::UnknownError(Box::new(e)))?;

        self.requests.push(request);
        Ok(self)
    }

    pub async fn fetch(&mut self) -> Result<LocalSheet, SpreadsheetCursorError> {
        let mut join_set = JoinSet::new();

        self.requests
            .drain(..)
            .map(|r| r.build(self.properties.clone()))
            .for_each(|f| {
                join_set.spawn(f);
            });

        let mut built_requests = vec![];
        while let Some(built_req) = join_set.join_next().await {
            let build_req = built_req
                .map_err(|e| SpreadsheetCursorError::UnknownError(Box::new(e)))?
                .map_err(|e| SpreadsheetCursorError::RequestError(e))?;

            built_requests.push(build_req);
        }

        todo!();
    }
}

pub enum SpreadsheetCursorError {
    UnknownError(Box<dyn std::error::Error>),
    RequestError(RequestError),
}

pub type Connection = s4::Sheets<HttpsConnector<HttpConnector>>;

pub struct SpreadsheetProperties {
    id: String,
    sheets: HashMap<String, SheetProperties>,
}

impl SpreadsheetProperties {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn sheet(&self, name: &str) -> Option<&SheetProperties> {
        self.sheets.get(name)
    }
}

pub struct SheetProperties {
    name: String,
    id: i32,
    columns: HashMap<String, usize>,
}

impl SheetProperties {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn id(&self) -> i32 {
        self.id
    }

    pub fn columns(&self) -> impl Iterator<Item = &String> {
        self.columns.keys()
    }

    pub fn column_idx(&self, name: &str) -> Option<&usize> {
        self.columns.get(name)
    }
}
/*

spreadsheet.commit(|c| {
    c.execute(req);
    c.execute(req2);
}).await?;

let entries: Vec<Vec<Value>> = spreadsheet.fetch(|c| {
    c.execute(req);
    c.execute(req2);
}).await?;

let entries: Vec<Sale> = spreadsheet.fetch(|c| {
    c.execute(from("sales").select("*").where(equal("a", 98).and) );
    c.execute(req2);
}).await?;


let cursor = spreadsheet.commit_cursor();

cursor.commit().await?;
*/
