use crate::{
    localize_callq,
    prelude::*,
    utils::{payload::PayloadOp, verify::verify_with_callback},
};
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, ReplyMarkup},
};

pub fn handler() -> HandlerResult {
    dptree::entry().branch(
        Update::filter_callback_query()
            .branch(dptree::filter(callback_prefix(PayloadOp::CancelOrder)).endpoint(order_cancel))
            .branch(
                dptree::filter(callback_prefix(PayloadOp::CompleteOrder)).endpoint(order_complete),
            )
            .branch(dptree::filter(callback_prefix(PayloadOp::PayOrder)).endpoint(order_pay)),
    )
}

pub async fn order_cancel(bot: Bot, q: CallbackQuery, warehouse: SharedWarehouse) -> Result<()> {
    let mut warehouse = warehouse.write().await;

    let Some(username) = q.from.username.clone() else {
        bot.answer_callback_query(q.id.clone())
            .text(localize_callq!(warehouse, &q, "No username"))
            .show_alert(true)
            .await?;
        return Ok(());
    };

    let order = verify_with_callback(&bot, &q, &mut warehouse)
        .payload_str_opt(&q.data)
        .await?
        .verify_order()
        .await?
        .stage_is_not(OrderStage::Cancelled)
        .await?
        .stage_is_not(OrderStage::Completed)
        .await?
        .participant_is(&username)
        .await?
        .update(|order| {
            order.stage = OrderStage::Cancelled;
        })
        .await?
        .branch(|v| async move {
            let amount = v.result().amount;
            // Rollback changes
            v.verify_product()
                .await?
                .update(|p| p.amount_left += amount)
                .await
        })
        .await?
        .branch(|v| async move {
            let order_id = v.result().id.clone();
            v.verify_customer()
                .await?
                .verify_meta()
                .await?
                .complete_order_by_id(&order_id)
                .await
        })
        .await?
        .branch(|v| async move {
            let order_id = v.result().id.clone();
            v.verify_merchant()
                .await?
                .verify_meta()
                .await?
                .complete_order_by_id(&order_id)
                .await
        })
        .await?
        .into_result();

    let other_participant_name = if username != order.merchant {
        order.merchant.clone()
    } else {
        order.customer.clone()
    };

    let other_participant_chat_id = verify_with_callback(&bot, &q, &mut warehouse)
        .user_meta_by_name(&other_participant_name)
        .await?
        .has_chat_id()
        .await?
        .into_result()
        .chat_id
        .unwrap();

    let item = verify_with_callback(&bot, &q, &mut warehouse)
        .item_by_id(&order.item_id)
        .await?
        .into_result();

    bot.send_message(
        other_participant_chat_id,
        localize_callq!(
            warehouse,
            &q,
            "We are sad to report, but your order for {name} has been canceled.",
            "name" => item.name
        ),
    )
    .reply_markup(ReplyMarkup::inline_kb(vec![vec![
        InlineKeyboardButton::switch_inline_query_current_chat(
            localize_callq!(warehouse, &q, "Details"),
            format!(".o {}", order.id),
        ),
    ]]))
    .await?;

    if let Some(msg) = q.message.clone() {
        bot.edit_message_reply_markup(msg.chat.id, msg.id)
            .reply_markup(InlineKeyboardMarkup::default())
            .await?;
    }

    bot.answer_callback_query(&q.id)
        .text(localize_callq!(
            warehouse,
            &q,
            "Order successfully cancelled."
        ))
        .await?;

    Ok(())
}

pub async fn order_complete(bot: Bot, q: CallbackQuery, warehouse: SharedWarehouse) -> Result<()> {
    let mut warehouse = warehouse.write().await;

    let Some(username) = q.from.username.clone() else {
        bot.answer_callback_query(&q.id)
            .text(localize_callq!(warehouse, &q, "No username"))
            .show_alert(true)
            .await?;
        return Ok(());
    };

    let order = verify_with_callback(&bot, &q, &mut warehouse)
        .payload_str_opt(&q.data)
        .await?
        .verify_order()
        .await?
        .stage_is_not(OrderStage::Cancelled)
        .await?
        .stage_is_not(OrderStage::Completed)
        .await?
        .merchant_is(&username)
        .await?
        .update(|order| {
            order.stage = OrderStage::Completed;
        })
        .await?
        .branch(|v| async move {
            let amount = v.result().amount;
            v.verify_product()
                .await?
                .update(|p| p.amount_sold += amount)
                .await
        })
        .await?
        .branch(|v| async move {
            let order_id = v.result().id.clone();
            v.verify_customer()
                .await?
                .verify_meta()
                .await?
                .complete_order_by_id(&order_id)
                .await
        })
        .await?
        .branch(|v| async move {
            let order_id = v.result().id.clone();
            v.verify_merchant()
                .await?
                .verify_meta()
                .await?
                .complete_order_by_id(&order_id)
                .await
        })
        .await?
        // Publish sale
        .branch(|v| async move { v.verfy_sale().await?.publish().await })
        .await?
        .into_result();

    let other_participant_name = if username != order.merchant {
        order.merchant.clone()
    } else {
        order.customer.clone()
    };

    let other_participant_chat_id = verify_with_callback(&bot, &q, &mut warehouse)
        .user_meta_by_name(&other_participant_name)
        .await?
        .has_chat_id()
        .await?
        .into_result()
        .chat_id
        .unwrap();

    let item = verify_with_callback(&bot, &q, &mut warehouse)
        .item_by_id(&order.item_id)
        .await?
        .into_result();

    bot.send_message(
        other_participant_chat_id,
        localize_callq!(
            warehouse,
            &q,
            "Your order for {name} has been successfully completed. Thank you for your purchase!",
            "name" => item.name
        ),
    )
    .reply_markup(ReplyMarkup::inline_kb(vec![vec![
        InlineKeyboardButton::switch_inline_query_current_chat(
            localize_callq!(warehouse, &q, "Details"),
            format!(".o {}", order.id),
        ),
    ]]))
    .await?;

    if let Some(msg) = q.message.clone() {
        bot.edit_message_reply_markup(msg.chat.id, msg.id)
            .reply_markup(InlineKeyboardMarkup::default())
            .await?;
    }

    bot.answer_callback_query(&q.id)
        .text(localize_callq!(
            warehouse,
            &q,
            "Order successfully completed."
        ))
        .await?;

    Ok(())
}

pub async fn order_pay(bot: Bot, q: CallbackQuery, warehouse: SharedWarehouse) -> Result<()> {
    let mut warehouse = warehouse.write().await;

    let Some(username) = q.from.username.clone() else {
        bot.answer_callback_query(&q.id)
            .text(localize_callq!(warehouse, &q, "No username"))
            .show_alert(true)
            .await?;
        return Ok(());
    };

    let order = verify_with_callback(&bot, &q, &mut warehouse)
        .payload_str_opt(&q.data)
        .await?
        .verify_order()
        .await?
        .stage_is(OrderStage::WaitForPayment)
        .await?
        .customer_is(&username)
        .await?
        .branch(|v| async move { v.verify_product().await?.supports_invoice().await })
        .await?
        .into_result();

    let customer_chat_id = verify_with_callback(&bot, &q, &mut warehouse)
        .user_meta_by_name(&order.customer)
        .await?
        .has_chat_id()
        .await?
        .into_result()
        .chat_id
        .unwrap();

    let customer = verify_with_callback(&bot, &q, &mut warehouse)
        .user_by_name(&order.customer)
        .await?
        .into_result();

    bot.answer_callback_query(q.id).await?;

    let chat_lang_code = q
        .from
        .language_code
        .as_ref()
        .map(|c| c.as_str())
        .unwrap_or("en");

    crate::dialogues::particular::purchase::invoice::send_invoice(
        bot,
        customer_chat_id,
        chat_lang_code,
        &mut warehouse,
        &customer.entry,
        order.entry,
    )
    .await?;

    Ok(())
}
