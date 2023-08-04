use chrono::Utc;
use regex::{Regex, RegexBuilder};
use tables::prelude::*;
use teloxide::{
    dispatching::dialogue::GetChatId,
    prelude::*,
    types::{
        CallbackQuery, ChatId, InlineQuery, KeyboardButton, KeyboardMarkup, Message, ReplyMarkup,
        Update,
    },
};

use lazy_static::lazy_static;

use crate::prelude::*;

pub async fn handle_user_from_inline(
    warehouse: &mut Warehouse,
    q: &InlineQuery,
) -> Result<(User, UserMeta)> {
    handle_user(warehouse, None, &q.from).await
}

pub async fn handle_user_from_msg(
    warehouse: &mut Warehouse,
    msg: &Message,
) -> Result<(User, UserMeta)> {
    let user = msg.from().ok_or(UnkError::unknown("upd.user"))?;
    handle_user(warehouse, msg.chat.id.into(), user).await
}

pub async fn handle_user_from_upd(
    warehouse: &mut Warehouse,
    upd: &Update,
) -> Result<(User, UserMeta)> {
    let user = upd.user().ok_or(UnkError::unknown("upd.user"))?;
    handle_user(warehouse, upd.chat_id(), user).await
}

pub async fn handle_user(
    warehouse: &mut Warehouse,
    chat_id: Option<ChatId>,
    user: &teloxide::types::User,
) -> Result<(User, UserMeta)> {
    handle_user_info(
        warehouse,
        user.username
            .as_ref()
            .ok_or(UnkError::unknown("upd.user.username"))?,
        chat_id,
        user.language_code.clone().unwrap_or("en".to_string()),
    )
    .await
}

pub async fn handle_user_info(
    warehouse: &mut Warehouse,
    username: &str,
    chat_id: Option<ChatId>,
    lang_code: String,
) -> Result<(User, UserMeta)> {
    warehouse.users.refresh().await?;
    warehouse.users_meta.refresh().await?;

    let username = username.to_owned();
    let user = warehouse.users.by_name.get_with_row(&username).cloned();
    let meta = warehouse
        .users_meta
        .by_name
        .get_with_row(&username)
        .cloned();

    Ok(match (user, meta) {
        (Some((user_row, mut user)), Some((meta_row, mut meta))) => {
            if meta.chat_id.is_none() && chat_id.is_some() {
                meta.chat_id = chat_id;
                warehouse.users_meta.update_one(meta_row, &meta).await?;
            };

            if user.lang_code != lang_code {
                user.lang_code = lang_code.clone();
                warehouse.users.update_one(user_row, &user).await?;
            };
            (user.clone(), meta.clone())
        }
        (_, _) => {
            let user = User {
                name: username.to_string(),
                role: Role::User,
                lang_code,
                created_date: Utc::now(),
                last_activity_date: Utc::now(),
                blocked: false,
            };

            let meta = UserMeta {
                name: username.to_string(),
                chat_id,
                pending_orders: vec![],
                completed_orders: vec![],
            };

            warehouse.users.extend_one(&user).await?;
            warehouse.users_meta.extend_one(&meta).await?;
            (user, meta)
        }
    })
}

pub async fn update_user_activity(warehouse: &mut Warehouse, username: &String) -> Result<()> {
    let (row, mut user) = warehouse
        .users
        .by_name
        .get_with_row(username)
        .ok_or(UnkError::tables("user doesn't exist"))?
        .clone();

    user.last_activity_date = Utc::now();
    warehouse.users.update_one(row, &user).await?;

    Ok(())
}

lazy_static! {
    pub static ref PRODUCT_ANS_RE: Regex = RegexBuilder::new(r"^.*\s\|\s(?<id>.*)$")
        .multi_line(true)
        .build()
        .unwrap();
}

pub fn make_product_answer(product: &Product, item: &Item) -> String {
    format!(
        "<b>{}</b> | <i>{}</i>\n{}",
        item.name,
        product.id(),
        &item.inline_desc
    )
}

pub fn filter_msg_prefix(prefix: &'static str) -> HandlerResult {
    dptree::entry().filter_async(move |msg: Message, warehouse: SharedWarehouse| async move {
        let mut warehouse = warehouse.write().await;

        if let Some(text) = msg.text() {
            text.starts_with(&crate::localize_msg!(warehouse, msg, prefix))
        } else {
            false
        }
    })
}

pub fn callback_prefix(text: impl ToString) -> impl Fn(CallbackQuery) -> bool {
    move |m: CallbackQuery| {
        m.data
            .map(|t| t.starts_with(&text.to_string()))
            .unwrap_or(false)
    }
}

pub async fn user_keyboard(warehouse: &mut Warehouse, lang_code: &str, user: &User) -> ReplyMarkup {
    let mut keyboard = vec![vec![
        KeyboardButton::new(crate::localize!(warehouse, lang_code, "🔍 Search").to_string()),
        KeyboardButton::new(crate::localize!(warehouse, lang_code, "📦 Orders").to_string()),
    ]];

    if user.role.is_at_least(Role::Merchant) {
        keyboard.push(vec![
            KeyboardButton::new(crate::localize!(warehouse, lang_code, "💸 Sell").to_string()),
            KeyboardButton::new(crate::localize!(
                warehouse,
                lang_code,
                "✍️ Writeoff".to_string()
            )),
        ]);
    }

    if user.role.is_at_least(Role::Moderator) {
        keyboard.push(vec![
            KeyboardButton::new(crate::localize!(
                warehouse,
                lang_code,
                "🪫 Replenish".to_string()
            )),
            KeyboardButton::new(crate::localize!(
                warehouse,
                lang_code,
                "🔄 Refresh".to_string()
            )),
        ]);
    }

    ReplyMarkup::Keyboard(KeyboardMarkup {
        keyboard,
        is_persistent: true,
        resize_keyboard: Some(true),
        input_field_placeholder: Some(crate::localize!(
            warehouse,
            lang_code,
            "@botname Search product..."
        )),
        ..Default::default()
    })
}

#[macro_export]
macro_rules! localize_msg {
    ($warehouse:expr, $msg:expr, $text:expr $(,$key:expr => $value:expr)*) => {
        $crate::localize!(
            $warehouse,
            match $msg.from() {
                Some(teloxide::types::User {
                    language_code: Some(lang_code),
                    ..
                }) => lang_code,
                _ => "en",
            },
            $text $(,$key => $value)*)
    };
}

#[macro_export]
macro_rules! localize_upd {
    ($warehouse:expr, $upd:expr, $text:expr $(,$key:expr => $value:expr)*) => {
        $crate::localize!(
            $warehouse,
            match $upd.user() {
                Some(teloxide::types::User {
                    language_code: Some(lang_code),
                    ..
                }) => lang_code,
                _ => "en",
            },
            $text $(,$key => $value)*)
    };
}

#[macro_export]
macro_rules! localize_callq {
    ($warehouse:expr, $q:expr, $text:expr $(,$key:expr => $value:expr)*) => {
        $crate::localize!(
            $warehouse,
            match $q.from {
                teloxide::types::User {
                    language_code: Some(ref lang_code),
                    ..
                } => lang_code,
                _ => "en",
            },
            $text $(,$key => $value)*)
    };
}

// Ignore entries::User's lang_code field until rewrite of tables
#[macro_export]
macro_rules! localize {
    ($warehouse:expr, $lang_code:expr, $text:expr) => {
        {
            let _ = $warehouse.localization.refresh().await;
            let loc_text = if let Some(loc) = $warehouse
                .localization
                .by_key_phrase
                .get(&$text.to_owned())
            {
                loc.get($lang_code)
            } else {
                let _ = $warehouse.localization.extend_one(&crate::entries::Localization {
                    key_phrase: $text.to_owned(),
                    en: "-".to_owned(),
                    ru: "-".to_owned()})
                    .await;
                $text.to_owned()
            };

            let map = std::collections::HashMap::<String, String>::new();
            strfmt::strfmt(&loc_text, &map).unwrap_or(loc_text)
        }
    };
    ($warehouse:expr, $lang_code:expr, $text:expr $(,$key:expr => $value:expr)*) => {
        {
            let _ = $warehouse.localization.refresh().await;
            let loc_text = if let Some(loc) = $warehouse
                .localization
                .by_key_phrase
                .get(&$text.to_string())
            {
                loc.get($lang_code)
            } else {
                let _ = $warehouse.localization.extend_one(&crate::entries::Localization {
                    key_phrase: $text.to_owned(),
                    en: "-".to_owned(),
                    ru: "-".to_owned()})
                    .await;
                $text.to_owned()
            };

            let mut map = std::collections::HashMap::<String, String>::new();
            $(map.insert($key.to_owned(), $value.to_string());)*
            strfmt::strfmt(&loc_text, &map).unwrap_or(loc_text)
        }
    };
}

pub async fn default_handler(bot: Bot, upd: Update, warehouse: SharedWarehouse) -> Result<()> {
    let mut warehouse = warehouse.write().await;

    match upd.kind {
        teloxide::types::UpdateKind::Message(ref msg) => {
            let (user, _) = handle_user_from_upd(&mut warehouse, &upd).await?;

            if msg.via_bot.is_some() {
                return Ok(());
            }

            bot.send_message(
                msg.chat.id,
                localize_msg!(
                    warehouse,
                    msg,
                    "Hi folks, find your dream product with the bot's test shop 🕶"
                ),
            )
            .reply_markup(user_keyboard(&mut warehouse, &user.lang_code, &user).await)
            .await?;
        }
        _ => (),
    }

    Ok(())
}
