use std::fmt::Display;

use crate::{utils::row::Row, BoxedError};

use super::*;

impl<'a, N: ErrorNotifier> VerifyDriver<'a, N> {
    pub async fn user_by_name(mut self, name: &str) -> Result<Verify<'a, N, Row<User>>> {
        let name = name.to_owned();

        match self.warehouse.users.refresh().await {
            Ok(_) => (),
            Err(e) => {
                self.notify("We are having technical difficulties, please try again later.")
                    .await?;
                return Err(Box::new(VerifyUserError::WarehouseRefreshError(Box::new(
                    e,
                ))));
            }
        }

        let user = match self.warehouse.users.by_name.get_with_row(&name) {
            Some(user) => user,
            None => {
                self.notify("Sorry, we can't find a user.").await?;
                return Err(Box::new(VerifyUserError::NotFound(name)));
            }
        }
        .clone();

        Ok(Verify {
            notifier: self.notifier,
            obj: user.into(),
            warehouse: self.warehouse,
        })
    }
}

impl<'a, N: ErrorNotifier> Verify<'a, N, Row<User>> {
    pub async fn meta_exists(self) -> Result<Verify<'a, N, Row<User>>> {
        let (user, driver) = self.split();

        Ok(driver
            .user_meta_by_name(&user.name)
            .await?
            .into_driver()
            .with(user))
    }
    pub async fn verify_meta(self) -> Result<Verify<'a, N, Row<UserMeta>>> {
        let (user, driver) = self.split();
        driver.user_meta_by_name(&user.name).await
    }

    pub async fn username_is_opt(
        mut self,
        username: Option<String>,
    ) -> Result<Verify<'a, N, Row<User>>> {
        if username.is_none() {
            self.notify("Sorry, we can't find a user with that name.")
                .await?;
            return Err(Box::new(VerifyUserError::WrongUsername(
                self.obj,
                "Option::None".to_owned(),
            )));
        }

        Ok(self.username_is(&username.unwrap()).await?)
    }

    pub async fn username_is(mut self, username: &str) -> Result<Verify<'a, N, Row<User>>> {
        if self.obj.name != username {
            self.notify("Sorry, we can't find a user with that name.")
                .await?;
            return Err(Box::new(VerifyUserError::WrongUsername(
                self.obj,
                username.to_owned(),
            )));
        }

        Ok(self)
    }
}

#[derive(Debug)]
pub enum VerifyUserError {
    WarehouseRefreshError(BoxedError),
    NotFound(String),
    WrongUsername(Row<User>, String),
}

impl Display for VerifyUserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyUserError::WarehouseRefreshError(e) => {
                write!(f, "Warehouse refresh error: {e}")
            }
            VerifyUserError::NotFound(name) => write!(f, "User with name {} not found", name),
            VerifyUserError::WrongUsername(user, name) => write!(
                f,
                "User with name {} not found, found user {} instead.\n{}:{:#?}",
                name, user.name, user.row, user.entry
            ),
        }
    }
}

impl std::error::Error for VerifyUserError {}
