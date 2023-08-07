use crate::{localize_msg, prelude::*};

use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, ReplyMarkup},
};

pub fn handler() -> HandlerResult {
    Update::filter_message()
        .branch(
            dptree::entry()
                .chain(filter_msg_prefix("/start"))
                .endpoint(start),
        )
        .branch(
            dptree::entry()
                .chain(filter_msg_prefix("📦 Orders"))
                .endpoint(orders),
        )
        .branch(
            dptree::entry()
                .chain(filter_msg_prefix("🔍 Search"))
                .endpoint(search),
        )
        .branch(
            dptree::entry()
                .chain(filter_msg_prefix("🔄 Refresh"))
                .endpoint(refresh),
        )
}

pub async fn start(bot: Bot, msg: Message, warehouse: SharedWarehouse) -> Result<()> {
    let mut warehouse = warehouse.write().await;
    let (user, _) = handle_user_from_msg(&mut warehouse, &msg).await?;

    let lang_code = msg
        .from()
        .map(|user| user.language_code.clone())
        .flatten()
        .unwrap_or("en".to_owned());

    bot.send_message(
        msg.chat.id,
        localize_msg!(
            warehouse,
            msg,
            "Hi folks, find your dream product with the bot's test shop 🕶"
        ),
    )
    .reply_markup(user_keyboard(&mut warehouse, &lang_code, &user).await)
    .await?;

    Ok(())
}

pub async fn orders(bot: Bot, msg: Message, warehouse: SharedWarehouse) -> Result<()> {
    let mut warehouse = warehouse.write().await;
    let (user, meta) = handle_user_from_msg(&mut warehouse, &msg).await?;

    if !user.role.is_at_least(Role::User) || user.blocked {
        return Ok(());
    }

    let active_cnt = meta.pending_orders.len();
    let compl_cnt = meta.completed_orders.len();

    let text = localize_msg!(warehouse, msg,
        concat!(
            "You have {active_cnt} active order{aend} and {compl_cnt} completed order{cend}. ",
            " You can also write \"@botname .o\" in the message bar to avoid invoking this command, or click button below."),
        "active_cnt" => active_cnt,
        "compl_cnt" => compl_cnt,
        "aend" => if active_cnt == 1 { "" } else { "s" },
        "cend" => if compl_cnt == 1 { "" } else { "s" }
    );

    bot.send_message(msg.chat.id, text)
        .reply_markup(ReplyMarkup::inline_kb(vec![vec![
            InlineKeyboardButton::switch_inline_query_current_chat(
                localize_msg!(warehouse, msg, "See orders..."),
                ".o ",
            ),
        ]]))
        .await?;

    Ok(())
}

pub async fn search(bot: Bot, msg: Message, warehouse: SharedWarehouse) -> Result<()> {
    let mut warehouse = warehouse.write().await;
    let (user, _) = handle_user_from_msg(&mut warehouse, &msg).await?;

    if !user.role.is_at_least(Role::User) || user.blocked {
        return Ok(());
    }

    let text = localize_msg!(
        warehouse,
        msg,
        concat!(
            "To search for products, use the inline mode of the bot with a ",
            "description of the product you need, for example write in the message bar \"@botname",
            " Hat\". ",
            "Or click the button right below this message!"
        )
    );

    bot.send_message(msg.chat.id, text)
        .reply_markup(ReplyMarkup::inline_kb(vec![vec![
            InlineKeyboardButton::switch_inline_query_current_chat(
                localize_msg!(warehouse, msg, "Search..."),
                "#1 ",
            ),
        ]]))
        .await?;

    Ok(())
}

pub async fn refresh(bot: Bot, msg: Message, warehouse: SharedWarehouse) -> Result<()> {
    let mut warehouse = warehouse.write().await;
    let (user, _) = handle_user_from_msg(&mut warehouse, &msg).await?;

    if !user.role.is_at_least(Role::Moderator) {
        return Ok(());
    }

    warehouse.products.inner.mark_as_dirty();
    warehouse.products.refresh().await?;
    warehouse.items.inner.mark_as_dirty();
    warehouse.items.refresh().await?;
    warehouse.users.inner.mark_as_dirty();
    warehouse.users.refresh().await?;
    warehouse.users_meta.inner.mark_as_dirty();
    warehouse.users_meta.refresh().await?;
    // warehouse.sales.mark_as_dirty();
    // warehouse.sales.refresh();
    warehouse.merchants.inner.mark_as_dirty();
    warehouse.merchants.refresh().await?;
    warehouse.orders.inner.mark_as_dirty();
    warehouse.orders.refresh().await?;
    warehouse.localization.inner.mark_as_dirty();
    warehouse.localization.refresh().await?;
    // warehouse.writeoffs.mark_as_dirty();
    // warehouse.replenishments.mark_as_dirty();

    bot.send_message(msg.chat.id, localize_msg!(warehouse, msg, "Done."))
        .await?;

    Ok(())
}
