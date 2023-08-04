use crate::common::*;
use crate::prelude::*;
use teloxide::dispatching::dialogue::{Dialogue, Storage};
use teloxide::prelude::*;

pub mod prelude {
    pub use super::{
        receive_amount_stage, receive_money_stage, receive_product_stage, receive_text_stage, start,
    };
}

pub async fn start<D, S>(
    bot: Bot,
    upd: Update,
    dialogue: Dialogue<D, S>,
    warehouse: SharedWarehouse,
) -> Result<()>
where
    D: ConversationStart + Send + Sync + 'static,
    S: Storage<D> + Send + Sync + 'static,
    S::Error: std::error::Error + Send + Sync,
{
    let mut warehouse = warehouse.write().await;
    let (user, meta) = handle_user_from_upd(&mut warehouse, &upd).await?;

    let stage = dialogue
        .get()
        .await?
        .ok_or(UnkError::dialogue("No dialogue stage"))?;

    if user.blocked || !user.role.is_at_least(stage.required_role()) {
        return Ok(());
    }

    dialogue
        .update(stage.start(bot, upd, (user, meta), &mut warehouse).await?)
        .await?;

    Ok(())
}

pub async fn receive_product_stage<D, S>(
    bot: Bot,
    msg: Message,
    dialogue: Dialogue<D, S>,
    warehouse: SharedWarehouse,
) -> Result<()>
where
    D: ConversationStart + ConversationStage<(Product, Item)> + Send + Sync + 'static,
    S: Storage<D> + Send + Sync + 'static,
    S::Error: std::error::Error + Send + Sync,
{
    let mut warehouse = warehouse.write().await;
    let (user, meta) = handle_user_from_msg(&mut warehouse, &msg).await?;

    let stage = dialogue
        .get()
        .await?
        .ok_or(UnkError::dialogue("No dialogue stage"))?;

    if user.blocked || !user.role.is_at_least(stage.required_role()) {
        return Ok(());
    }

    let (_, product) = match receive_product(bot.clone(), &msg, &mut warehouse).await? {
        Some((row, product)) => (*row, product.clone()),
        None => return Ok(()),
    };

    warehouse.items.refresh().await?;

    let item = match warehouse.items.by_id.get(&product.item_id) {
        Some(item) => item.clone(),
        None => {
            bot.send_message(
                msg.chat.id,
                "Product was edited during the dialogue. Try to choose another one.",
            )
            .await?;
            return Ok(());
        }
    };

    dialogue
        .update(
            stage
                .next(
                    bot.clone(),
                    msg,
                    (user, meta),
                    &mut warehouse,
                    (product, item),
                )
                .await?,
        )
        .await?;

    Ok(())
}

pub async fn receive_product<'a>(
    bot: Bot,
    msg: &Message,
    warehouse: &'a mut Warehouse,
) -> Result<Option<(&'a usize, &'a Product)>> {
    let text = match msg.text() {
        Some(t) => t,
        None => {
            bot.send_message(msg.chat.id, "Please send a text.").await?;
            return Ok(None);
        }
    };

    let id = match PRODUCT_ANS_RE.captures(text) {
        Some(c) => c.name("id").unwrap().as_str().parse::<u64>()?,
        None => {
            bot.send_message(msg.chat.id, "Wrong product, please try again.")
                .await?;
            return Ok(None);
        }
    };

    fetch_product(bot.clone(), msg, warehouse, id).await
}

pub async fn verify_product<'a>(
    bot: Bot,
    msg: &Message,
    warehouse: &'a mut Warehouse,
    product: &Product,
) -> Result<Option<(&'a usize, &'a Product)>> {
    warehouse.products.refresh().await?;

    match warehouse.products.by_id.get_with_row(&product.id()) {
        Some((row, product)) => Ok(Some((row, product))),
        None => {
            bot.send_message(msg.chat.id, "Product was removed during the dialogue. ")
                .await?;
            Ok(None)
        }
    }
}

pub async fn fetch_product<'a>(
    bot: Bot,
    msg: &Message,
    warehouse: &'a mut Warehouse,
    id: u64,
) -> Result<Option<(&'a usize, &'a Product)>> {
    warehouse.products.refresh().await?;

    let (row, product) = match warehouse.products.by_id.get_with_row(&id) {
        Some((row, product)) => (row, product),
        None => {
            bot.send_message(msg.chat.id, "Product not found, please try again.")
                .await?;
            return Ok(None);
        }
    };

    Ok(Some((row, product)))
}

pub async fn receive_amount_stage<D, S>(
    bot: Bot,
    msg: Message,
    dialogue: Dialogue<D, S>,
    warehouse: SharedWarehouse,
) -> Result<()>
where
    D: ConversationStart + ConversationStage<u32> + Send + Sync + 'static,
    S: Storage<D> + Send + Sync + 'static,
    S::Error: std::error::Error + Send + Sync,
{
    let mut warehouse = warehouse.write().await;
    let (user, meta) = handle_user_from_msg(&mut warehouse, &msg).await?;

    let stage = dialogue
        .get()
        .await?
        .ok_or(UnkError::dialogue("No dialogue stage"))?;

    if user.blocked || !user.role.is_at_least(stage.required_role()) {
        return Ok(());
    }

    let amount = match receive_amount(bot.clone(), &msg).await? {
        Some(p) => p,
        None => return Ok(()),
    };

    dialogue
        .update(
            stage
                .next(bot.clone(), msg, (user, meta), &mut warehouse, amount)
                .await?,
        )
        .await?;

    Ok(())
}

pub async fn receive_amount(bot: Bot, msg: &Message) -> Result<Option<u32>> {
    let text = match msg.text() {
        Some(t) => t,
        None => {
            bot.send_message(msg.chat.id, "Please send a number.")
                .await?;
            return Ok(None);
        }
    };

    Ok(match text.parse::<u32>() {
        Ok(amount) => match amount {
            0 => {
                bot.send_message(msg.chat.id, "🧐").await?;
                bot.send_message(msg.chat.id, "Please send a positive number.")
                    .await?;
                None
            }
            _ => Some(amount),
        },
        Err(_) => {
            bot.send_message(msg.chat.id, "Invalid number format.")
                .await?;
            None
        }
    })
}

pub async fn receive_money_stage<D, S>(
    bot: Bot,
    msg: Message,
    dialogue: Dialogue<D, S>,
    warehouse: SharedWarehouse,
) -> Result<()>
where
    D: ConversationStart + ConversationStage<(f64, Currency)> + Send + Sync + 'static,
    S: Storage<D> + Send + Sync + 'static,
    S::Error: std::error::Error + Send + Sync,
{
    let mut warehouse = warehouse.write().await;
    let (user, meta) = handle_user_from_msg(&mut warehouse, &msg).await?;

    let stage = dialogue
        .get()
        .await?
        .ok_or(UnkError::dialogue("No dialogue stage"))?;

    if user.blocked || !user.role.is_at_least(stage.required_role()) {
        return Ok(());
    }

    let (money, currency) = match receive_money(bot.clone(), &msg).await? {
        Some(p) => p,
        None => return Ok(()),
    };

    dialogue
        .update(
            stage
                .next(
                    bot.clone(),
                    msg,
                    (user, meta),
                    &mut warehouse,
                    (money, currency),
                )
                .await?,
        )
        .await?;

    Ok(())
}

pub async fn receive_money(bot: Bot, msg: &Message) -> Result<Option<(f64, Currency)>> {
    let text = match msg.text() {
        Some(t) => t.trim().to_uppercase(),
        None => {
            bot.send_message(msg.chat.id, "Please send a number with a currency.")
                .await?;
            return Ok(None);
        }
    };

    let mut parts = text.split(' ');

    let price = match parts.next() {
        Some(text) => match text.parse::<f64>() {
            Ok(price) => price,
            Err(e) => {
                bot.send_message(msg.chat.id, format!("Worng number format: {e}"))
                    .await?;
                return Ok(None);
            }
        },
        None => {
            bot.send_message(msg.chat.id, "Wrong format.").await?;
            return Ok(None);
        }
    };

    let currency = match parts.next() {
        Some(text) => match Currency::parse(text) {
            Ok(c) => c,
            Err(e) => {
                bot.send_message(msg.chat.id, format!("Wrong currency format: {e}"))
                    .await?;
                return Ok(None);
            }
        },
        None => {
            bot.send_message(msg.chat.id, "Wrong currency format.")
                .await?;
            return Ok(None);
        }
    };

    Ok(Some((price, currency)))
}

pub async fn receive_text_stage<D, S>(
    bot: Bot,
    msg: Message,
    dialogue: Dialogue<D, S>,
    warehouse: SharedWarehouse,
) -> Result<()>
where
    D: ConversationStart + ConversationStage<String> + Send + Sync + 'static,
    S: Storage<D> + Send + Sync + 'static,
    S::Error: std::error::Error + Send + Sync,
{
    let mut warehouse = warehouse.write().await;
    let (user, meta) = handle_user_from_msg(&mut warehouse, &msg).await?;

    let stage = dialogue
        .get()
        .await?
        .ok_or(UnkError::dialogue("No dialogue stage"))?;

    if user.blocked || !user.role.is_at_least(stage.required_role()) {
        return Ok(());
    }

    let text = match msg.text() {
        Some(t) => t.to_owned(),
        None => {
            bot.send_message(msg.chat.id, "Please send a text.").await?;
            return Ok(());
        }
    };

    let stage = dialogue
        .get()
        .await?
        .ok_or(UnkError::dialogue("No dialogue stage"))?;

    dialogue
        .update(
            stage
                .next(
                    bot.clone(),
                    msg,
                    (user, meta),
                    &mut warehouse,
                    text.to_string(),
                )
                .await?,
        )
        .await?;

    Ok(())
}
