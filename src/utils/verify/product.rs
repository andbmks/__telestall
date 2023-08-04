use std::fmt::Display;

use crate::{utils::row::Row, BoxedError};

use super::*;

impl<'a, N: ErrorNotifier> VerifyDriver<'a, N> {
    pub async fn product_by_id(mut self, id: ProductId) -> Result<Verify<'a, N, Row<Product>>> {
        match self.warehouse.products.refresh().await {
            Ok(_) => (),
            Err(e) => {
                self.notify("We are having technical difficulties, please try again later.")
                    .await?;
                return Err(Box::new(VerifyProductError::WarehouseRefreshError(
                    Box::new(e),
                )));
            }
        }

        let product = match self.warehouse.products.by_id.get_with_row(&id) {
            Some(product) => product,
            None => {
                self.notify("Sorry, we can't find your product.").await?;
                return Err(Box::new(VerifyProductError::NotFound(id)));
            }
        }
        .clone();

        Ok(Verify {
            notifier: self.notifier,
            obj: product.into(),
            warehouse: self.warehouse,
        })
    }
}

impl<'a, N: ErrorNotifier> Verify<'a, N, Row<Product>> {
    pub async fn update<Fn: FnOnce(&mut Row<Product>)>(
        mut self,
        upd: Fn,
    ) -> Result<Verify<'a, N, Row<Product>>> {
        upd(&mut self.obj);

        let result = self
            .warehouse
            .products
            .update_one(self.obj.row, &self.obj.entry)
            .await;

        if result.is_err() {
            self.notify("We are unable to update your order. Please try again later.")
                .await?;

            return Err(Box::new(VerifyProductError::WarehouseUpdateError(
                Box::new(result.unwrap_err()),
            )));
        }

        Ok(self)
    }
    pub async fn item_exists(self) -> Result<Verify<'a, N, Row<Product>>> {
        let (product, driver) = self.split();

        Ok(driver
            .item_by_id(&product.item_id)
            .await?
            .into_driver()
            .with(product))
    }

    pub async fn verify_item(self) -> Result<Verify<'a, N, Row<Item>>> {
        let (product, driver) = self.split();
        driver.item_by_id(&product.item_id).await
    }

    pub async fn verify_merchant(self) -> Result<Verify<'a, N, Row<User>>> {
        let (product, driver) = self.split();
        driver.user_by_name(&product.merchant).await
    }

    pub async fn left_at_least(mut self, amount: u32) -> Result<Verify<'a, N, Row<Product>>> {
        if self.obj.amount_left < amount {
            self.notify("Sorry, we don't have enough of this product.")
                .await?;

            return Err(Box::new(VerifyProductError::NotEnough(self.obj, amount)));
        }

        Ok(self)
    }

    pub async fn visible_to_user(mut self, user: &User) -> Result<Verify<'a, N, Row<Product>>> {
        if !self.obj.is_visible_to(user) {
            self.notify("I'm sorry, that product is missing.").await?;

            return Err(Box::new(VerifyProductError::InvisibleForUser(
                self.obj,
                user.clone(),
            )));
        }

        Ok(self)
    }

    pub async fn visible_to_username_opt(
        mut self,
        username: &Option<String>,
    ) -> Result<Verify<'a, N, Row<Product>>> {
        if let Some(username) = username {
            self.visible_to_username(username).await
        } else {
            self.notify("I'm sorry, that product is missing.").await?;
            Err(Box::new(VerifyProductError::NoUsername(self.obj)))?
        }
    }

    pub async fn visible_to_username(self, username: &str) -> Result<Verify<'a, N, Row<Product>>> {
        let product = self.obj.clone();
        let (user, mut driver) = self.into_driver().user_by_name(username).await?.split();

        if !product.is_visible_to(&user.entry) {
            driver.notify("I'm sorry, that product is missing.").await?;

            return Err(Box::new(VerifyProductError::InvisibleForUser(
                product, user.entry,
            )));
        }

        Ok(driver.with(product))
    }

    pub async fn merchant_is(
        mut self,
        username: impl ToString,
    ) -> Result<Verify<'a, N, Row<Product>>> {
        let username = username.to_string();

        if self.obj.merchant != username {
            self.notify("Sorry, you are not the seller of this product.")
                .await?;

            return Err(Box::new(VerifyProductError::InvalidMerchant(
                self.obj, username,
            )));
        }

        Ok(self)
    }

    pub async fn merchant_is_not(
        mut self,
        username: impl ToString,
    ) -> Result<Verify<'a, N, Row<Product>>> {
        let username = username.to_string();

        if self.obj.merchant == username {
            self.notify("Sorry, you can't do that as a seller of a product.")
                .await?;

            return Err(Box::new(VerifyProductError::InvalidMerchant(
                self.obj, username,
            )));
        }

        Ok(self)
    }

    pub async fn supports_invoice(mut self) -> Result<Verify<'a, N, Row<Product>>> {
        if !self.obj.supports_invoice() {
            self.notify(concat!(
                "Sorry, product doesn't support invoices. ",
                "The product may have changed over time, pleae try again."
            ))
            .await?;

            return Err(Box::new(VerifyProductError::InvoiceUnsupported(self.obj)));
        }

        Ok(self)
    }

    pub async fn currency_is(mut self, currency: Currency) -> Result<Verify<'a, N, Row<Product>>> {
        if self.obj.currency != currency {
            self.notify(concat!(
                "Sorry, wrong currency. ",
                "The product may have changed over time, please try again."
            ))
            .await?;

            return Err(Box::new(VerifyProductError::WrongCurrency(self.obj)));
        }

        Ok(self)
    }

    pub async fn price_is(mut self, price: f64) -> Result<Verify<'a, N, Row<Product>>> {
        if self.obj.price != price {
            self.notify(concat!(
                "Sorry, wrong price. ",
                "The product may have changed over time, please try again."
            ))
            .await?;

            return Err(Box::new(VerifyProductError::WrongPrice(self.obj)));
        }

        Ok(self)
    }

    pub async fn price_is_not_negotiated(mut self) -> Result<Verify<'a, N, Row<Product>>> {
        if self.obj.negotiated_price {
            self.notify(concat!("Sorry, the price of this product is negotiated. ",))
                .await?;

            return Err(Box::new(VerifyProductError::WrongPrice(self.obj)));
        }
        Ok(self)
    }
}

#[derive(Debug)]
pub enum VerifyProductError {
    WarehouseRefreshError(BoxedError),
    WarehouseUpdateError(BoxedError),
    NotFound(ProductId),
    NotEnough(Row<Product>, u32),
    NoUsername(Row<Product>),
    InvisibleForUser(Row<Product>, User),
    InvalidMerchant(Row<Product>, String),
    InvoiceUnsupported(Row<Product>),
    WrongCurrency(Row<Product>),
    WrongPrice(Row<Product>),
}

impl Display for VerifyProductError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyProductError::WarehouseRefreshError(e) => {
                write!(f, "Warehouse refresh error: {e}")
            }
            VerifyProductError::WarehouseUpdateError(e) => {
                write!(f, "Warehouse update error: {e}")
            }
            VerifyProductError::NotFound(id) => {
                write!(f, "Product with id {} not found", id)
            }
            VerifyProductError::NotEnough(product, amount) => {
                write!(
                    f,
                    "Product {} has only {} left, but {} was requested. \n{}:{:#?}",
                    product.id(),
                    product.amount_left,
                    amount,
                    product.row,
                    product.entry
                )
            }
            VerifyProductError::NoUsername(product) => {
                write!(
                    f,
                    "Product {} invisible beacuse of None username. \n{}:{:#?}",
                    product.id(),
                    product.row,
                    product.entry
                )
            }
            VerifyProductError::InvisibleForUser(product, user) => {
                write!(
                    f,
                    "Product {} is not visible for user {}. \n{}:{:#?} \n{:#?}",
                    product.id(),
                    user.name,
                    product.row,
                    product.entry,
                    user
                )
            }
            VerifyProductError::InvalidMerchant(pair, username) => {
                write!(
                    f,
                    "Product {} is not owned by user {}. \n{}:{:#?}",
                    pair.id(),
                    username,
                    pair.row,
                    pair.entry
                )
            }
            VerifyProductError::InvoiceUnsupported(pair) => {
                write!(
                    f,
                    "Product {} does not support invoice. \n{}:{:#?}",
                    pair.id(),
                    pair.row,
                    pair.entry
                )
            }
            VerifyProductError::WrongCurrency(pair) => {
                write!(
                    f,
                    "Product {} has wrong currency. \n{}:{:#?}",
                    pair.id(),
                    pair.row,
                    pair.entry
                )
            }
            VerifyProductError::WrongPrice(pair) => {
                write!(
                    f,
                    "Product {} has wrong price. \n{}:{:#?}",
                    pair.id(),
                    pair.row,
                    pair.entry
                )
            }
        }
    }
}

impl std::error::Error for VerifyProductError {}
