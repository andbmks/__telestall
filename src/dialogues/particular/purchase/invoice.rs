use super::*;

pub fn handler() -> HandlerResult {
    dptree::entry()
        // Handle pre-checkout query
        .branch(
            Update::filter_pre_checkout_query()
                .filter(|q: PreCheckoutQuery| PayloadOp::Checkout.is_in_payload(&q.invoice_payload))
                .endpoint(pre_checkout),
        )
        // Handle successful payment message
        .branch(
            Update::filter_message()
                .filter(|msg: Message| {
                    if let Some(successful_payment) = msg.successful_payment() {
                        PayloadOp::Checkout.is_in_payload(&successful_payment.invoice_payload)
                    } else {
                        false
                    }
                })
                .endpoint(successful_payment),
        )
}

pub async fn send_invoice(
    bot: Bot,
    chat_id: ChatId,
    lang_code: &str,
    warehouse: &mut Warehouse,
    user: &User,
    order: Order,
) -> Result<()> {
    let item = verify_with_chat_user(&bot, chat_id, user, lang_code, warehouse)
        .item_by_id(&order.item_id)
        .await?
        .into_result();

    bot.send_message(
        chat_id,
        localize!(
            warehouse,
            lang_code,
            concat!(
                "Here's your invoice for the purchase of <b>{name}</b> in quantit{end} of <b>{quantity}</b>. ",
                "You can get an invoice at any time through the order menu."
            ),
            "name" => localize!(warehouse, lang_code, item.name),
            "end" => if order.amount == 1 { "y" } else { "ies" },
            "quantity" => order.amount
        ),
    )
    .parse_mode(ParseMode::Html)
    .await?;

    let result = bot
        .send_invoice(
            chat_id,
            localize!(warehouse, lang_code, item.name),
            localize!(warehouse, lang_code, item.full_desc),
            Payload::checkout(order.id).to_string(),
            localize!(warehouse, lang_code, "PROVIDER_TOKEN"),
            order.currency.to_string(),
            vec![LabeledPrice::new(
                format!("{}x {}", order.amount, item.name.clone()),
                (order.cost * 100.0) as i32,
            )],
        )
        .photo_url(item.image_url.clone().parse()?)
        .await;

    match result {
        Ok(_) => (),
        Err(e) => {
            error!("Failed to send invoice: {}", e);
            bot.send_message(
                chat_id,
                localize!(
                    warehouse,
                    &lang_code,
                    concat!(
                        "Failed to send invoice. You may have reached the price limit for a ",
                        "single purchase or there may have been an internal error."
                    )
                ),
            )
            .await?;
        }
    }

    Ok(())
}

async fn pre_checkout(bot: Bot, q: PreCheckoutQuery, warehouse: SharedWarehouse) -> Result<()> {
    let mut warehouse = warehouse.write().await;

    let _ = verify_with_pre_checkout(&bot, &q, &mut warehouse)
        .payload_str(&q.invoice_payload)
        .await?
        .branch(|v| async move {
            v.verify_order()
                .await?
                .stage_is(OrderStage::WaitForPayment)
                .await?
                .branch(|v| async move {
                    v.verify_customer()
                        .await?
                        .username_is_opt(q.from.username)
                        .await?
                        .verify_meta()
                        .await
                })
                .await?
                .customer_has_order()
                .await?
                .merchant_has_order()
                .await?
                .currency_is(q.currency)
                .await?
                .cost_is(q.total_amount as f64 / 100.0)
                .await
        });

    bot.answer_pre_checkout_query(q.id, true).await?;

    Ok(())
}

async fn successful_payment(bot: Bot, msg: Message, warehouse: SharedWarehouse) -> Result<()> {
    if msg.successful_payment().is_none() {
        return Ok(());
    }

    let mut warehouse = warehouse.write().await;
    let payment = msg.successful_payment().unwrap();

    let order = verify_with_msg(&bot, &msg, &mut warehouse)
        .payload_str(payment.invoice_payload.as_str())
        .await?
        .verify_order()
        .await?
        .update(|o| {
            o.stage = OrderStage::Paid;
        })
        .await?
        .into_result();

    let merchant_chat_id = verify_with_msg(&bot, &msg, &mut warehouse)
        .user_meta_by_name(&order.merchant)
        .await?
        .has_chat_id()
        .await?
        .into_result()
        .chat_id
        .unwrap();

    bot.send_message(
        msg.chat.id,
        [
            format!(
                "Thank you for your payment! The seller @{} will be in ",
                order.merchant
            )
            .as_str(),
            "touch with you soon, but if you have any questions, you can ask him yourself.",
        ]
        .join(""),
    )
    .parse_mode(ParseMode::Html)
    .reply_markup(ReplyMarkup::inline_kb(vec![vec![
        InlineKeyboardButton::switch_inline_query_current_chat(
            "Details",
            format!(".o {}", order.id),
        ),
    ]]))
    .await?;

    let item = verify_with_msg(&bot, &msg, &mut warehouse)
        .item_by_id(&order.item_id)
        .await?
        .into_result();

    bot.send_message(
        merchant_chat_id,
        format!("The order for {} has been paid!", item.name),
    )
    .reply_markup(ReplyMarkup::inline_kb(vec![vec![
        InlineKeyboardButton::switch_inline_query_current_chat(
            "Details",
            format!(".o {}", order.id),
        ),
    ]]))
    .await?;

    Ok(())
}
