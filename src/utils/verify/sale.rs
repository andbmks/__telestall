use std::fmt::Display;

use crate::{utils::row::Row, BoxedError};

use super::*;

impl<'a, N: ErrorNotifier> Verify<'a, N, Row<Sale>> {
    pub async fn publish(mut self) -> Result<Verify<'a, N, Row<Sale>>> {
        let result = self.warehouse.sales.extend_one(&self.obj).await;

        if result.is_err() {
            self.notify("We are unable to publish your sale. Please try again later.")
                .await?;

            return Err(Box::new(VerifySaleError::WarehouseUpdateError(Box::new(
                result.unwrap_err(),
            ))));
        }
        Ok(self)
    }
}

#[derive(Debug)]
pub enum VerifySaleError {
    WarehouseUpdateError(BoxedError),
}

impl Display for VerifySaleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifySaleError::WarehouseUpdateError(e) => {
                write!(f, "Warehouse update error: {e}")
            }
        }
    }
}

impl std::error::Error for VerifySaleError {}
