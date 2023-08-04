use std::{error::Error as StdError, fmt::Display, str::FromStr};

use google_sheets4::api::GridRange;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};

lazy_static! {
    pub static ref A1_RE: Regex = Regex::new(
        r"^(?<name>.*)!(?<c_start>[A-Z]+)(?<r_start>([1-9]\d*)?)(:(?<c_end>[A-Z]+)(?<r_end>([1-9]\d*)?))?$"
    ).unwrap();
}

#[derive(Debug)]
pub enum Error {
    InvalidRange(String),
    InvalidRangeBounds(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InvalidRange(s) => f.write_str(&format!("invalid range: {}", s)),
            Error::InvalidRangeBounds(s) => f.write_str(&format!("invalid range bounds: {}", s)),
        }
    }
}

impl StdError for Error {}

#[derive(Default, Clone)]
pub struct SheetRange {
    // In the near future it can become a generic to support sheet_index
    pub sheet_name: String,
    pub c_start: usize,
    pub r_start: usize,
    pub c_end: usize,
    pub r_end: Option<usize>,
}

impl SheetRange {
    pub fn with_rows(&self, r_start: usize, r_end: usize) -> Self {
        Self {
            r_start: r_start,
            r_end: Some(r_end),
            c_start: self.c_start,
            c_end: self.c_end,
            sheet_name: self.sheet_name.clone(),
        }
    }

    pub fn with_cols(&self, c_start: usize, c_end: usize) -> Self {
        Self {
            c_start,
            c_end,
            r_start: self.r_start,
            r_end: self.r_end,
            sheet_name: self.sheet_name.clone(),
        }
    }

    pub fn with_inf_end(&self) -> Self {
        Self {
            r_end: None,
            c_end: self.c_end,
            r_start: self.r_start,
            c_start: self.c_start,
            sheet_name: self.sheet_name.clone(),
        }
    }

    pub fn as_grid_range(&self, sheet_id: i32) -> GridRange {
        GridRange {
            sheet_id: Some(sheet_id),
            start_column_index: Some(self.c_start as i32),
            end_column_index: Some(self.c_end as i32),
            start_row_index: Some(self.r_start as i32),
            end_row_index: self.r_end.map(|r| r as i32),
        }
    }
}

impl ToString for SheetRange {
    fn to_string(&self) -> String {
        let from_col = |col| {
            std::iter::repeat('A')
                .take((col / 26) as usize)
                .chain([((col % 26) as u8 + 'A' as u8) as char].into_iter())
                .collect::<String>()
        };

        format!(
            "{}!{}{}:{}{}",
            self.sheet_name,
            from_col(self.c_start),
            self.r_start + 1,
            from_col(self.c_end - 1),
            self.r_end.map(|r| r.to_string()).unwrap_or("".to_owned())
        )
    }
}

impl FromStr for SheetRange {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        let to_col = |s: regex::Match<'_>| {
            s.as_str()
                .chars()
                .fold(0, |acc, c| acc * 26 + (c as u8 - 'A' as u8 + 1) as usize)
        };

        let to_row = |s: regex::Match<'_>| s.as_str().parse::<usize>().unwrap();

        let captures = A1_RE.captures(s).ok_or(Error::InvalidRange(s.to_owned()))?;
        let sheet_name = captures.name("name").unwrap().as_str().to_owned();
        let c_start = captures.name("c_start").map(|s| to_col(s)).unwrap() - 1;
        let r_start = captures
            .name("r_start")
            .filter(|s| !s.is_empty())
            .map(|s| to_row(s))
            .unwrap_or(1)
            - 1;

        let (c_end, r_end) = match captures.name("c_end") {
            // If there is a column end and no row end, then the row end is unbounded
            Some(s) => {
                let c_end = to_col(s);
                let r_end = captures
                    .name("r_end")
                    .filter(|s| !s.is_empty())
                    .map(|s| to_row(s));
                (c_end, r_end)
            }
            // If there is no column end, then the range is poiting to a cell
            None => {
                let c_end = c_start + 1;
                let r_end = Some(r_start + 1);
                (c_end, r_end)
            }
        };

        if c_start > c_end || r_end.is_some_and(|r_end| r_end < r_start) {
            return Err(Error::InvalidRange(s.to_owned()));
        }

        Ok(Self {
            sheet_name,
            c_start,
            c_end,
            r_start,
            r_end,
        })
    }
}

impl Serialize for SheetRange {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SheetRange {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(|e| serde::de::Error::custom(e))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn range_full_from_a1() {
        let range = SheetRange::from_str("Sheet!A2:D5").unwrap();

        assert_eq!(range.sheet_name, "Sheet");
        assert_eq!(range.c_start, 0);
        assert_eq!(range.c_end, 4);
        assert_eq!(range.r_start, 1);
        assert_eq!(range.r_end, Some(5));
    }

    #[test]
    fn range_inf_end_from_a1() {
        let range = SheetRange::from_str("Sheet!A2:D").unwrap();

        assert_eq!(range.sheet_name, "Sheet");
        assert_eq!(range.c_start, 0);
        assert_eq!(range.c_end, 4);
        assert_eq!(range.r_start, 1);
        assert_eq!(range.r_end, None);
    }

    #[test]
    fn range_col_only_from_a1() {
        let range = SheetRange::from_str("Sheet!A:D").unwrap();

        assert_eq!(range.sheet_name, "Sheet");
        assert_eq!(range.c_start, 0);
        assert_eq!(range.c_end, 4);
        assert_eq!(range.r_start, 0);
        assert_eq!(range.r_end, None);
    }

    #[test]
    fn cell_pos_from_a1() {
        let range = SheetRange::from_str("Sheet!A1").unwrap();

        assert_eq!(range.sheet_name, "Sheet");
        assert_eq!(range.c_start, 0);
        assert_eq!(range.c_end, 1);
        assert_eq!(range.r_start, 0);
        assert_eq!(range.r_end, Some(1));
    }

    #[test]
    fn range_full_to_a1() {
        let range = SheetRange {
            sheet_name: "Sheet".to_owned(),
            c_start: 1,
            r_start: 1,
            c_end: 5,
            r_end: Some(5),
        };

        assert_eq!(range.to_string(), "Sheet!B2:E5");
    }
}
