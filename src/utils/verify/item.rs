use std::fmt::Display;

use crate::{utils::row::Row, BoxedError};

use super::*;

impl<'a, N: ErrorNotifier> VerifyDriver<'a, N> {
    pub async fn item_by_id(mut self, id: &str) -> Result<Verify<'a, N, Row<Item>>> {
        let id = id.to_owned();

        match self.warehouse.items.refresh().await {
            Ok(_) => (),
            Err(e) => {
                self.notify("We are having technical difficulties, please try again later.")
                    .await?;
                return Err(Box::new(VerifyItemError::WarehouseRefreshError(Box::new(
                    e,
                ))));
            }
        }

        let item = match self.warehouse.items.by_id.get_with_row(&id) {
            Some(product) => product,
            None => {
                self.notify("Sorry, we can't find your item.").await?;
                return Err(Box::new(VerifyItemError::NotFound(id)));
            }
        }
        .clone();

        Ok(Verify {
            notifier: self.notifier,
            obj: item.into(),
            warehouse: self.warehouse,
        })
    }
}

#[derive(Debug)]
pub enum VerifyItemError {
    WarehouseRefreshError(BoxedError),
    NotFound(String),
}

impl Display for VerifyItemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyItemError::WarehouseRefreshError(e) => {
                write!(f, "Warehouse refresh error: {e}")
            }
            VerifyItemError::NotFound(id) => {
                write!(f, "Item with id {} not found", id)
            }
        }
    }
}

impl std::error::Error for VerifyItemError {}
