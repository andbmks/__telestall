pub mod item;
pub mod order;
pub mod payload;
pub mod product;
pub mod sale;
pub mod user;
pub mod user_meta;

use crate::prelude::*;
use async_trait::async_trait;
use futures::Future;
use teloxide::{prelude::*, types::ParseMode};

pub mod prelude {
    pub use super::item::*;
    pub use super::order::*;
    pub use super::payload::*;
    pub use super::product::*;
    pub use super::{
        verify_with_callback, verify_with_chat, verify_with_chat_user, verify_with_pre_checkout,
    };
}

pub struct VerifyDriver<'a, N> {
    notifier: N,
    warehouse: &'a mut Warehouse,
}

impl<'a, N, T> From<Verify<'a, N, T>> for VerifyDriver<'a, N> {
    fn from(verify: Verify<'a, N, T>) -> Self {
        verify.into_driver()
    }
}

impl<'a, N> VerifyDriver<'a, N> {
    pub fn with<T>(self, obj: T) -> Verify<'a, N, T> {
        Verify {
            notifier: self.notifier,
            warehouse: self.warehouse,
            obj,
        }
    }
}

impl<'a, N: ErrorNotifier> VerifyDriver<'a, N> {
    pub async fn notify(&mut self, err: &str) -> Result<()> {
        self.notifier.notify(self.warehouse, err).await
    }
}

pub struct Verify<'a, N, T> {
    notifier: N,
    warehouse: &'a mut Warehouse,
    obj: T,
}

impl<'a, N, T> Verify<'a, N, T> {
    pub const fn result(&self) -> &T {
        &self.obj
    }

    pub fn into_result(self) -> T {
        self.obj
    }

    pub fn into_driver(self) -> VerifyDriver<'a, N> {
        VerifyDriver {
            notifier: self.notifier,
            warehouse: self.warehouse,
        }
    }

    pub fn split(self) -> (T, VerifyDriver<'a, N>) {
        (
            self.obj,
            VerifyDriver {
                notifier: self.notifier,
                warehouse: self.warehouse,
            },
        )
    }
}

impl<'a, N, T: Clone> Verify<'a, N, T> {
    pub async fn branch<
        Fn: FnOnce(Self) -> F,
        F: Future<Output = Result<D>>,
        D: Into<VerifyDriver<'a, N>>,
    >(
        self,
        exec: Fn,
    ) -> Result<Verify<'a, N, T>> {
        let obj = self.obj.clone();
        let driver = exec(self).await?.into();

        Ok(Verify {
            notifier: driver.notifier,
            warehouse: driver.warehouse,
            obj,
        })
    }
}

impl<'a, N: ErrorNotifier, T> Verify<'a, N, T> {
    pub async fn notify(&mut self, err: &str) -> Result<()> {
        self.notifier.notify(self.warehouse, err).await
    }
}

#[async_trait]
pub trait ErrorNotifier {
    async fn notify(&self, warehouse: &mut Warehouse, err: &str) -> Result<()>;
}

pub struct BotMessageWithKbNotifier<'a> {
    bot: &'a Bot,
    user: &'a User,
    lang_code: &'a str,
    chat_id: ChatId,
}

#[async_trait]
impl<'a> ErrorNotifier for BotMessageWithKbNotifier<'a> {
    async fn notify(&self, warehouse: &mut Warehouse, err: &str) -> Result<()> {
        self.bot
            .send_message(self.chat_id, err)
            .parse_mode(ParseMode::Html)
            .reply_markup(user_keyboard(warehouse, self.lang_code, self.user).await)
            .await?;
        Ok(())
    }
}

pub fn verify_with_chat_user<'a>(
    bot: &'a Bot,
    chat_id: ChatId,
    user: &'a User,
    lang_code: &'a str,
    warehouse: &'a mut Warehouse,
) -> VerifyDriver<'a, BotMessageWithKbNotifier<'a>> {
    VerifyDriver {
        notifier: BotMessageWithKbNotifier {
            bot,
            user,
            lang_code,
            chat_id,
        },
        warehouse,
    }
}

pub struct BotMessageNotifier<'a> {
    bot: &'a Bot,
    chat_id: ChatId,
    lang_code: String,
}

#[async_trait]
impl<'a> ErrorNotifier for BotMessageNotifier<'a> {
    async fn notify(&self, warehouse: &mut Warehouse, err: &str) -> Result<()> {
        self.bot
            .send_message(self.chat_id, localize!(warehouse, &self.lang_code, err))
            .parse_mode(ParseMode::Html)
            .await?;
        Ok(())
    }
}

pub fn verify_with_msg<'a>(
    bot: &'a Bot,
    msg: &Message,
    warehouse: &'a mut Warehouse,
) -> VerifyDriver<'a, BotMessageNotifier<'a>> {
    let lang_code = msg
        .from()
        .map(|m| m.language_code.clone())
        .flatten()
        .unwrap_or("en".to_owned());

    verify_with_chat(bot, msg.chat.id, lang_code, warehouse)
}

pub fn verify_with_chat<'a>(
    bot: &'a Bot,
    chat_id: ChatId,
    lang_code: String,
    warehouse: &'a mut Warehouse,
) -> VerifyDriver<'a, BotMessageNotifier<'a>> {
    VerifyDriver {
        notifier: BotMessageNotifier {
            bot,
            lang_code,
            chat_id,
        },
        warehouse,
    }
}

pub struct BotCallbackNotifier<'a> {
    bot: &'a Bot,
    id: String,
    lang_code: String,
}

#[async_trait]
impl<'a> ErrorNotifier for BotCallbackNotifier<'a> {
    async fn notify(&self, warehouse: &mut Warehouse, err: &str) -> Result<()> {
        self.bot
            .answer_callback_query(self.id.clone())
            .text(localize!(warehouse, &self.lang_code, err))
            .show_alert(true)
            .await?;
        Ok(())
    }
}

pub fn verify_with_callback<'a>(
    bot: &'a Bot,
    q: &CallbackQuery,
    warehouse: &'a mut Warehouse,
) -> VerifyDriver<'a, BotCallbackNotifier<'a>> {
    let lang_code = q.from.language_code.clone().unwrap_or("en".to_owned());
    VerifyDriver {
        notifier: BotCallbackNotifier {
            bot,
            id: q.id.clone(),
            lang_code,
        },
        warehouse,
    }
}

pub struct BotPreCheckoutNotifier<'a> {
    bot: &'a Bot,
    id: String,
    lang_code: String,
}

#[async_trait]
impl<'a> ErrorNotifier for BotPreCheckoutNotifier<'a> {
    async fn notify(&self, warehouse: &mut Warehouse, err: &str) -> Result<()> {
        self.bot
            .answer_pre_checkout_query(&self.id, false)
            .error_message(localize!(warehouse, &self.lang_code, err))
            .await?;
        Ok(())
    }
}

pub fn verify_with_pre_checkout<'a>(
    bot: &'a Bot,
    q: &PreCheckoutQuery,
    warehouse: &'a mut Warehouse,
) -> VerifyDriver<'a, BotPreCheckoutNotifier<'a>> {
    let lang_code = q.from.language_code.clone().unwrap_or("en".to_owned());

    VerifyDriver {
        notifier: BotPreCheckoutNotifier {
            bot,
            id: q.id.clone(),
            lang_code,
        },
        warehouse,
    }
}
