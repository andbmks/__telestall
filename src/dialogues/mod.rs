pub mod particular;
pub mod stages;

use crate::prelude::*;
use async_trait::async_trait;
use log::error;
use std::sync::Arc;
use teloxide::dispatching::dialogue::{Dialogue, Storage};
use teloxide::dispatching::DpHandlerDescription;
use teloxide::prelude::*;
use teloxide::types::UpdateKind;

pub mod prelude {
    pub use super::{
        cancel, filter_dialogue_started, stages::prelude::*, ConversationEnd, ConversationStage,
        ConversationStart,
    };
}

pub fn handler() -> HandlerResult {
    particular::handler()
}

pub fn write_deps(deps: &mut DependencyMap) {
    particular::write_deps(deps)
}

#[async_trait]
pub trait ConversationStart {
    fn is_started(&self) -> bool;

    fn required_role(&self) -> Role {
        Role::User
    }

    async fn start(
        self,
        bot: Bot,
        upd: Update,
        user: (User, UserMeta),
        warehouse: &mut Warehouse,
    ) -> Result<Self>
    where
        Self: Sized;
}

#[async_trait]
pub trait ConversationStage<T> {
    async fn next(
        self,
        bot: Bot,
        msg: Message,
        user: (User, UserMeta),
        warehouse: &mut Warehouse,
        item: T,
    ) -> Result<Self>
    where
        Self: Sized;
}

#[async_trait]
pub trait ConversationEnd {
    async fn end(
        self,
        bot: Bot,
        msg: Message,
        user: (User, UserMeta),
        warehouse: &mut Warehouse,
    ) -> Result<()>
    where
        Self: Sized;
}

pub fn filter_dialogue_started<D, S>() -> HandlerResult
where
    D: ConversationStart + Send + Sync + 'static,
    S: Storage<D> + Send + Sync + 'static,
    S::Error: std::error::Error + Send + Sync,
{
    dptree::filter_async(|dialogue: Dialogue<D, S>| async move {
        match dialogue.get().await {
            Ok(stage) => match stage {
                Some(stage) => stage.is_started(),
                _ => false,
            },
            Err(_) => false,
        }
    })
}

pub async fn cancel<D, S>(
    bot: Bot,
    msg: Message,
    warehouse: SharedWarehouse,
    dialogue: Dialogue<D, S>,
) -> Result<()>
where
    D: Send + Sync + 'static,
    S: Storage<D> + Send + Sync + 'static,
    S::Error: std::error::Error + Send + Sync,
{
    dialogue.exit().await?;

    let mut warehouse = warehouse.write().await;
    let (user, _) = handle_user_from_msg(&mut warehouse, &msg).await?;

    let lang_code = msg
        .from()
        .map(|u| u.language_code.clone())
        .flatten()
        .unwrap_or("en".to_owned());

    bot.send_message(msg.chat.id, "Dialogue cancelled.")
        .reply_markup(user_keyboard(&mut warehouse, &lang_code, &user).await)
        .await?;
    Ok(())
}

pub fn enter_user_dialogue<S, D>(
    err_msg: &'static str,
) -> Handler<'static, DependencyMap, Result<()>, DpHandlerDescription>
where
    S: Storage<D> + Sized + Send + Sync + 'static,
    <S as Storage<D>>::Error: std::fmt::Debug + Send,
    D: Default + Send + Sync + 'static,
{
    dptree::entry()
        .filter_map_async(
            move |bot: Bot, upd: Update, warehouse: SharedWarehouse, storage: Arc<S>| async move {
                let mut warehouse = warehouse.write().await;
                let _ = warehouse.users.refresh().await;
                let _ = warehouse.users_meta.refresh().await;

                let user = match upd.user() {
                    Some(user) => user,
                    None => {
                        error!("Update has no user");
                        return None;
                    }
                };

                let username = match &user.username {
                    Some(username) => username,
                    None => {
                        error!("User has no username");
                        return None;
                    }
                };

                match warehouse.users.refresh().await {
                    Ok(_) => (),
                    Err(e) => {
                        error!("Failed to refresh users: {}", e);
                        return None;
                    }
                }

                let err_msg = localize_upd!(warehouse, upd, err_msg);

                let result = warehouse.users_meta.by_name.get_with_row(&username);
                match result {
                    Some((_, meta)) => match meta.chat_id {
                        Some(chat_id) => Some(Dialogue::new(storage, chat_id)),
                        None => {
                            answer_err(bot, upd, err_msg.to_owned()).await;
                            None
                        }
                    },
                    None => {
                        error!("Query callback user not found in the user table.");
                        answer_err(bot, upd, err_msg.to_owned()).await;
                        None
                    }
                }
            },
        )
        .filter_map_async(|dialogue: Dialogue<D, S>| async move {
            match dialogue.get_or_default().await {
                Ok(dialogue) => Some(dialogue),
                Err(err) => {
                    log::error!("dialogue.get_or_default() failed: {:?}", err);
                    None
                }
            }
        })
}

async fn answer_err(bot: Bot, upd: Update, text: String) {
    match upd.kind {
        UpdateKind::CallbackQuery(q) => {
            let _ = bot
                .answer_callback_query(q.id)
                .text(text)
                .show_alert(true)
                .send()
                .await;
        }
        _ => (),
    }
}
