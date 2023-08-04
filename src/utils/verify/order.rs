use std::fmt::{Debug, Display};

use crate::{utils::row::Row, BoxedError};

use super::*;

impl<'a, N: ErrorNotifier> VerifyDriver<'a, N> {
    pub async fn order_by_id(mut self, id: OrderId) -> Result<Verify<'a, N, Row<Order>>> {
        match self.warehouse.orders.refresh().await {
            Ok(_) => (),
            Err(e) => {
                self.notify("We are having technical difficulties, please try again later.")
                    .await?;
                return Err(Box::new(VerifyOrderError::WarehouseRefreshError(Box::new(
                    e,
                ))));
            }
        }

        let order = match self.warehouse.orders.by_id.get_with_row(&id) {
            Some(order) => order,
            None => {
                self.notify("Sorry, we can't find your order.").await?;
                return Err(Box::new(VerifyOrderError::NotFound(id)));
            }
        }
        .clone();

        Ok(Verify {
            notifier: self.notifier,
            obj: order.into(),
            warehouse: self.warehouse,
        })
    }
}

impl<'a, N: ErrorNotifier> Verify<'a, N, Row<Order>> {
    pub async fn update<Fn: FnOnce(&mut Row<Order>)>(
        mut self,
        upd: Fn,
    ) -> Result<Verify<'a, N, Row<Order>>> {
        upd(&mut self.obj);

        let result = self
            .warehouse
            .orders
            .update_one(self.obj.row, &self.obj.entry)
            .await;

        if result.is_err() {
            self.notify("We are unable to update your order. Please try again later.")
                .await?;

            return Err(Box::new(VerifyOrderError::WarehouseUpdateError(Box::new(
                result.unwrap_err(),
            ))));
        }

        Ok(self)
    }

    pub async fn verify_product(self) -> Result<Verify<'a, N, Row<Product>>> {
        let product_id = self.obj.product_id();
        self.into_driver().product_by_id(product_id).await
    }

    pub async fn verfy_sale(self) -> Result<Verify<'a, N, Row<Sale>>> {
        let (order, driver) = self.split();

        let (product, driver) = driver.with(order.clone()).verify_product().await?.split();

        let sale = order.entry.into_sale(product.share);

        Ok(driver.with(Row::new(0, sale)))
    }

    pub async fn stage_is_not(mut self, stage: OrderStage) -> Result<Verify<'a, N, Row<Order>>> {
        if self.obj.stage == stage {
            self.notify(&format!(
                "Sorry, this order is already in the {:?} stage.",
                stage
            ))
            .await?;

            return Err(Box::new(VerifyOrderError::WrongStage(self.obj, stage)));
        }

        Ok(self)
    }

    pub async fn stage_is(mut self, stage: OrderStage) -> Result<Verify<'a, N, Row<Order>>> {
        if self.obj.stage != stage {
            self.notify(&format!(
                "Sorry, this order is not in the {:?} stage.",
                stage
            ))
            .await?;

            return Err(Box::new(VerifyOrderError::WrongStage(self.obj, stage)));
        }

        Ok(self)
    }

    pub async fn participant_is(mut self, username: &str) -> Result<Verify<'a, N, Row<Order>>> {
        if self.obj.customer != username && self.obj.merchant != username {
            self.notify("Sorry, you are not a participant in this order.")
                .await?;

            return Err(Box::new(VerifyOrderError::NotParticipant(
                self.obj,
                username.to_string(),
            )));
        }

        Ok(self)
    }

    pub async fn verify_customer(self) -> Result<Verify<'a, N, Row<User>>> {
        let (order, driver) = self.split();
        driver.user_by_name(&order.customer).await
    }

    pub async fn customer_is(
        mut self,
        username: impl ToString,
    ) -> Result<Verify<'a, N, Row<Order>>> {
        let username = username.to_string();

        if self.obj.customer != username {
            self.notify("I'm sorry, this order is not yours.").await?;

            return Err(Box::new(VerifyOrderError::InvalidMerchant(
                self.obj, username,
            )));
        }

        Ok(self)
    }

    pub async fn customer_is_not(
        mut self,
        username: impl ToString,
    ) -> Result<Verify<'a, N, Row<Order>>> {
        let username = username.to_string();

        if self.obj.customer != username {
            self.notify("Sorry, you can't do this in the buyer role.")
                .await?;

            return Err(Box::new(VerifyOrderError::InvalidMerchant(
                self.obj, username,
            )));
        }

        Ok(self)
    }

    pub async fn verify_merchant(self) -> Result<Verify<'a, N, Row<User>>> {
        let (order, driver) = self.split();
        driver.user_by_name(&order.merchant).await
    }

    pub async fn merchant_is(
        mut self,
        username: impl ToString,
    ) -> Result<Verify<'a, N, Row<Order>>> {
        let username = username.to_string();

        if self.obj.merchant != username {
            self.notify("Sorry, you are not the seller.").await?;

            return Err(Box::new(VerifyOrderError::InvalidMerchant(
                self.obj, username,
            )));
        }

        Ok(self)
    }

    pub async fn merchant_is_not(
        mut self,
        username: impl ToString,
    ) -> Result<Verify<'a, N, Row<Order>>> {
        let username = username.to_string();

        if self.obj.merchant != username {
            self.notify("Sorry, you can't do that as a seller of a product.")
                .await?;

            return Err(Box::new(VerifyOrderError::InvalidMerchant(
                self.obj, username,
            )));
        }

        Ok(self)
    }

    pub async fn currency_is(mut self, currency: Currency) -> Result<Verify<'a, N, Row<Order>>> {
        if self.obj.currency != currency {
            self.notify(concat!(
                "Sorry, wrong currency. ",
                "The product may have changed over time, please try again."
            ))
            .await?;

            return Err(Box::new(VerifyOrderError::WrongCurrency(self.obj)));
        }

        Ok(self)
    }

    pub async fn cost_is(mut self, cost: f64) -> Result<Verify<'a, N, Row<Order>>> {
        if self.obj.cost != cost {
            self.notify(concat!(
                "Sorry, wrong price. ",
                "The product may have changed over time, please try again."
            ))
            .await?;

            return Err(Box::new(VerifyOrderError::WrongCost(self.obj)));
        }

        Ok(self)
    }

    pub async fn merchant_has_order(self) -> Result<Verify<'a, N, Row<Order>>> {
        let (order, driver) = self.split();
        Ok(driver
            .with(order.clone())
            .stage_is(OrderStage::WaitForPayment)
            .await?
            .verify_merchant()
            .await?
            .verify_meta()
            .await?
            .has_pending_order_id(&order.id)
            .await?
            .into_driver()
            .with(order))
    }

    pub async fn customer_has_order(self) -> Result<Verify<'a, N, Row<Order>>> {
        let (order, driver) = self.split();
        Ok(driver
            .with(order.clone())
            .stage_is(OrderStage::WaitForPayment)
            .await?
            .verify_customer()
            .await?
            .verify_meta()
            .await?
            .has_pending_order_id(&order.id)
            .await?
            .into_driver()
            .with(order))
    }
}

#[derive(Debug)]
pub enum VerifyOrderError {
    WarehouseRefreshError(BoxedError),
    WarehouseUpdateError(BoxedError),
    NotFound(OrderId),
    WrongStage(Row<Order>, OrderStage),
    NotParticipant(Row<Order>, String),
    InvalidCustomer(Row<Order>, String),
    InvalidMerchant(Row<Order>, String),
    WrongCurrency(Row<Order>),
    WrongCost(Row<Order>),
}

impl Display for VerifyOrderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyOrderError::WarehouseRefreshError(e) => {
                write!(f, "Warehouse refresh error: {e}")
            }
            VerifyOrderError::WarehouseUpdateError(e) => {
                write!(f, "Warehouse update error: {e}")
            }
            VerifyOrderError::NotFound(id) => {
                write!(f, "Order not found: {}", id)
            }
            VerifyOrderError::WrongStage(order, stage) => {
                write!(
                    f,
                    "Order in the wrong {:?} stage. {}:{:#?}",
                    stage, order.row, order.entry
                )
            }
            VerifyOrderError::NotParticipant(order, username) => {
                write!(
                    f,
                    "Not a participant in the order {}. {}:{:#?}",
                    username, order.row, order.entry
                )
            }
            VerifyOrderError::InvalidCustomer(order, username) => {
                write!(
                    f,
                    "Invalid order customer {}. {}:{:#?}",
                    username, order.row, order.entry
                )
            }
            VerifyOrderError::InvalidMerchant(order, username) => {
                write!(
                    f,
                    "Invalid order merchant {}. {}:{:#?}",
                    username, order.row, order.entry
                )
            }
            VerifyOrderError::WrongCurrency(order) => {
                write!(f, "Wrong currency. {}:{:#?}", order.row, order.entry)
            }
            VerifyOrderError::WrongCost(order) => {
                write!(f, "Wrong cost. {}:{:#?}", order.row, order.entry)
            }
        }
    }
}

impl std::error::Error for VerifyOrderError {}
