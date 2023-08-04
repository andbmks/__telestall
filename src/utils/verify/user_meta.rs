use std::fmt::Display;

use crate::{utils::row::Row, BoxedError};

use super::*;

impl<'a, N: ErrorNotifier> VerifyDriver<'a, N> {
    pub async fn user_meta_by_name(mut self, name: &str) -> Result<Verify<'a, N, Row<UserMeta>>> {
        let name = name.to_owned();

        match self.warehouse.users_meta.refresh().await {
            Ok(_) => (),
            Err(e) => {
                self.notify("We are having technical difficulties, please try again later.")
                    .await?;
                return Err(Box::new(VerifyUserMetaError::WarehouseRefreshError(
                    Box::new(e),
                )));
            }
        }

        let user_meta = match self.warehouse.users_meta.by_name.get_with_row(&name) {
            Some(user_meta) => user_meta,
            None => {
                self.notify("Sorry, we can't find a user.").await?;
                return Err(Box::new(VerifyUserMetaError::NotFound(name)));
            }
        }
        .clone();

        Ok(Verify {
            notifier: self.notifier,
            obj: user_meta.into(),
            warehouse: self.warehouse,
        })
    }
}

impl<'a, N: ErrorNotifier> Verify<'a, N, Row<UserMeta>> {
    pub async fn has_chat_id(mut self) -> Result<Verify<'a, N, Row<UserMeta>>> {
        if self.obj.chat_id.is_none() {
            self.notify("Sorry, we can't find chat ID.").await?;
            return Err(Box::new(VerifyUserMetaError::NoChatId(self.obj)));
        }

        Ok(self)
    }

    pub async fn complete_order_by_id(
        self,
        order_id: &OrderId,
    ) -> Result<Verify<'a, N, Row<UserMeta>>> {
        let mut _self = self.has_pending_order_id(&order_id).await?;

        let pos = _self
            .obj
            .pending_orders
            .iter()
            .position(|other_id| other_id == order_id)
            .unwrap();

        let order_id = _self.obj.pending_orders.swap_remove(pos);
        _self.obj.completed_orders.push(order_id);

        let result = _self
            .warehouse
            .users_meta
            .update_one(_self.obj.row, &_self.obj.entry)
            .await;

        if result.is_err() {
            _self
                .notify("We are unable to update your order. Please try again later.")
                .await?;

            return Err(Box::new(VerifyUserMetaError::WarehouseUpdateError(
                Box::new(result.unwrap_err()),
            )));
        }

        Ok(_self)
    }

    pub async fn has_pending_order_id(
        mut self,
        order_id: &OrderId,
    ) -> Result<Verify<'a, N, Row<UserMeta>>> {
        if !self.obj.pending_orders.contains(order_id) {
            self.notify("Sorry, we can't find a pending order with that ID.")
                .await?;
            return Err(Box::new(VerifyUserMetaError::NoPendingOrder(
                self.obj,
                order_id.to_string(),
            )));
        }

        Ok(self)
    }
}

#[derive(Debug)]
pub enum VerifyUserMetaError {
    WarehouseRefreshError(BoxedError),
    WarehouseUpdateError(BoxedError),
    NotFound(String),
    NoChatId(Row<UserMeta>),
    NoPendingOrder(Row<UserMeta>, String),
}

impl Display for VerifyUserMetaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyUserMetaError::WarehouseRefreshError(e) => {
                write!(f, "Warehouse refresh error: {}", e)
            }
            VerifyUserMetaError::WarehouseUpdateError(e) => {
                write!(f, "Warehouse update error: {}", e)
            }
            VerifyUserMetaError::NotFound(name) => write!(f, "User with name {} not found", name),
            VerifyUserMetaError::NoChatId(user_meta) => write!(
                f,
                "User {} does not have a chat ID.\n{}:{:#?}",
                user_meta.name, user_meta.row, user_meta.entry
            ),
            VerifyUserMetaError::NoPendingOrder(user_meta, order_id) => write!(
                f,
                "User {} does not have a pending order with ID {}\n{}:{:#?}",
                user_meta.name, order_id, user_meta.row, user_meta.entry
            ),
        }
    }
}

impl std::error::Error for VerifyUserMetaError {}
