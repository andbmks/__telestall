use std::{
    fmt::{Debug, Display},
    ops::Range,
};

use crate::{
    utils::{payload::Payload, row::Row},
    BoxedError,
};

use super::*;

impl<'a, N: ErrorNotifier> VerifyDriver<'a, N> {
    pub async fn payload_str_opt(
        mut self,
        content: &Option<String>,
    ) -> Result<Verify<'a, N, Payload>> {
        if content.is_none() {
            self.notify("Sorry, something went wrong, try again.")
                .await?;
            return Err(Box::new(VerifyPayloadError::PayloadEmpty));
        }

        Ok(self.payload_str(content.as_ref().unwrap()).await?)
    }

    pub async fn payload_str(mut self, content: &str) -> Result<Verify<'a, N, Payload>> {
        let payload = match content.parse::<Payload>() {
            Ok(payload) => payload,
            Err(e) => {
                self.notify("Sorry, something went wrong, try again.")
                    .await?;
                return Err(Box::new(VerifyPayloadError::PayloadParseError(Box::new(e))));
            }
        };

        Ok(Verify {
            notifier: self.notifier,
            obj: payload,
            warehouse: self.warehouse,
        })
    }
}

impl<'a, N: ErrorNotifier> Verify<'a, N, Payload> {
    pub async fn has_product_id(mut self) -> Result<Verify<'a, N, Payload>> {
        if self.obj.product_id.is_none() {
            self.notify("Sorry, we can't identify your item, please try again.")
                .await?;
            return Err(Box::new(VerifyPayloadError::NoProductId(self.obj)));
        };

        Ok(self)
    }

    pub async fn verify_product(mut self) -> Result<Verify<'a, N, Row<Product>>> {
        let Some(product_id) = self.obj.product_id else {
            self
                .notify("Sorry, we can't identify your item, please try again.")
                .await?;
            return Err(Box::new(VerifyPayloadError::NoProductId(self.obj)));
        };

        Ok(self.into_driver().product_by_id(product_id).await?)
    }

    pub async fn has_order_id(mut self) -> Result<Verify<'a, N, Payload>> {
        if self.obj.order_id.is_none() {
            self.notify("Sorry, we can't identify your order, please try again.")
                .await?;
            return Err(Box::new(VerifyPayloadError::NoProductId(self.obj)));
        };

        Ok(self)
    }

    pub async fn verify_order(self) -> Result<Verify<'a, N, Row<Order>>> {
        let (payload, mut driver) = self.split();

        let Some(order_id) = payload.order_id else {
            driver
                .notify("Sorry, we can't identify your order, please try again.")
                .await?;
            return Err(Box::new(VerifyPayloadError::NoOrderId(payload)));
        };

        Ok(driver.order_by_id(order_id).await?)
    }

    pub async fn amount_min_product_left(self) -> Result<Verify<'a, N, Payload>> {
        let (payload, driver) = self.split();
        let (product, driver) = driver.with(payload.clone()).verify_product().await?.split();

        driver
            .with(payload)
            .amount_in_range(0..(product.amount_left + 1))
            .await
    }

    pub async fn amount_in_range(self, range: Range<u32>) -> Result<Verify<'a, N, Payload>> {
        let mut _self = self.has_amount().await?;

        if !range.contains(&_self.obj.amount.unwrap()) {
            _self.notify("Incorrect amount, please try again").await?;
            return Err(Box::new(VerifyPayloadError::NoAmount(_self.obj)));
        }

        Ok(_self)
    }

    pub async fn has_amount(mut self) -> Result<Verify<'a, N, Payload>> {
        if self.obj.amount.is_none() {
            self.notify("We can't determine the quantity, please try again.")
                .await?;

            return Err(Box::new(VerifyPayloadError::NoAmount(self.obj)));
        }
        Ok(self)
    }
}

#[derive(Debug)]
pub enum VerifyPayloadError {
    PayloadEmpty,
    PayloadParseError(BoxedError),
    NoProductId(Payload),
    NoOrderId(Payload),
    NoAmount(Payload),
    AmountNotInRange(Payload, Range<u32>),
}

impl Display for VerifyPayloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyPayloadError::PayloadEmpty => write!(f, "Payload is empty"),
            VerifyPayloadError::PayloadParseError(e) => {
                write!(f, "Payload parse error: {}", e)
            }
            VerifyPayloadError::NoProductId(payload) => {
                write!(f, "No product id in payload: {:#?}", payload)
            }
            VerifyPayloadError::NoOrderId(payload) => {
                write!(f, "No order id in payload: {:#?}", payload)
            }
            VerifyPayloadError::NoAmount(payload) => {
                write!(f, "No amount in payload: {:#?}", payload)
            }
            VerifyPayloadError::AmountNotInRange(payload, range) => {
                write!(
                    f,
                    "Amount not in range: {:#?}, range: {:#?}",
                    payload, range
                )
            }
        }
    }
}

impl std::error::Error for VerifyPayloadError {}
