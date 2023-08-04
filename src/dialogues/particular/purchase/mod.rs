pub mod invoice;

use async_trait::async_trait;
use chrono::Utc;
use log::error;
use teloxide::{
    dispatching::dialogue::InMemStorage,
    prelude::*,
    types::{
        InlineKeyboardButton, KeyboardButton, KeyboardMarkup, LabeledPrice, ParseMode, ReplyMarkup,
        Update, UpdateKind,
    },
};

use crate::utils::verify::{prelude::*, verify_with_msg};
use crate::{dialogues::enter_user_dialogue, prelude::*, utils::row::Row};
use crate::{
    localize_upd,
    utils::payload::{Payload, PayloadOp},
};

type Storage = InMemStorage<Stage>;

#[derive(Default, Clone)]
struct StageData {
    pub product: Option<Row<Product>>,
    pub item: Option<Row<Item>>,
    pub amount: Option<u32>,
    pub payment_method: Option<PurchaseWith>,
}

#[derive(Default, Clone)]
enum Stage {
    #[default]
    Start,
    WaitAmount(StageData),
    WaitPaymentMethod(StageData),
    WaitConfirm(StageData),
}

#[derive(Clone)]
enum PurchaseWith {
    Cash,
    Card,
    Negotiated,
}

pub fn handler() -> HandlerResult {
    dptree::entry()
        // Handle purcahse
        .branch(
            Update::filter_callback_query()
                .chain(enter_user_dialogue::<Storage, Stage>(
                    "To purchase a product you first need to start a dialog with the bot."
                ))
                .filter(callback_prefix(PayloadOp::Purchase))
                .endpoint(start::<Stage, Storage>),
        )
        // Handle dialogue stages
        .branch(
            Update::filter_message()
                .enter_dialogue::<Message, Storage, Stage>()
                .branch(
                    filter_dialogue_started::<Stage, Storage>()
                        .chain(filter_msg_prefix("Cancel"))
                        .endpoint(cancel::<Stage, Storage>),
                )
                .branch(
                    dptree::case![Stage::WaitAmount(data)]
                        .endpoint(receive_amount_stage::<Stage, Storage>),
                )
                .branch(
                    dptree::case![Stage::WaitPaymentMethod(data)]
                        .endpoint(receive_text_stage::<Stage, Storage>),
                )
                .branch(
                    dptree::case![Stage::WaitConfirm(data)]
                        .endpoint(receive_text_stage::<Stage, Storage>),
                ),
        )
        .chain(invoice::handler())
}

pub fn write_deps(deps: &mut DependencyMap) {
    deps.insert(InMemStorage::<Stage>::new());
}

#[async_trait]
impl ConversationStart for Stage {
    fn is_started(&self) -> bool {
        match self {
            Self::Start => false,
            _ => true,
        }
    }

    fn required_role(&self) -> Role {
        Role::User
    }

    async fn start(
        self,
        bot: Bot,
        upd: Update,
        user: (User, UserMeta),
        warehouse: &mut Warehouse,
    ) -> Result<Self> {
        match (self, upd.clone()) {
            (
                Self::Start,
                Update {
                    kind: UpdateKind::CallbackQuery(q),
                    ..
                },
            ) => {
                let payload_content = q.data.clone().unwrap_or("".to_owned());

                let chat_id = user.1.chat_id.unwrap();
                let product = verify_with_callback(&bot, &q, warehouse)
                    .payload_str(&payload_content)
                    .await?
                    .verify_product()
                    .await?
                    .visible_to_user(&user.0)
                    .await?
                    .left_at_least(1)
                    .await?
                    .into_result();

                let item = verify_with_callback(&bot, &q, warehouse)
                    .item_by_id(&product.item_id)
                    .await?
                    .into_result();

                bot.send_message(chat_id, localize_upd!(
                        warehouse, upd, 
                        "Hey, you wanted to buy the <b>{name}</b>, I'm very pleased! Just need to clarify how much you want to buy?",
                        "name" => localize_upd!(warehouse, upd, item.name) 
                    ))
                    .parse_mode(ParseMode::Html)
                    .reply_markup(ReplyMarkup::Keyboard(KeyboardMarkup {
                        resize_keyboard: Some(true),
                        is_persistent: true,
                        keyboard: vec![
                            (1..=5)
                                .map(|i| KeyboardButton::new(i.to_string()))
                                .collect(),
                            vec![KeyboardButton::new(localize_upd!(warehouse, upd, "Cancel"))],
                        ],
                        ..Default::default()
                    }))
                    .await?;

                bot.answer_callback_query(q.id.clone()).await?;

                Ok(Self::WaitAmount(StageData {
                    product: Some(product),
                    item: Some(item),
                    ..Default::default()
                }))
            }
            _ => Ok(Self::Start),
        }
    }
}

#[async_trait]
impl ConversationStage<u32> for Stage {
    async fn next(
        self,
        bot: Bot,
        msg: Message,
        user: (User, UserMeta),
        warehouse: &mut Warehouse,
        amount: u32,
    ) -> Result<Self> {
        match self {
            Stage::WaitAmount(mut data) => {
                data.amount = Some(amount as u32);

                let product = data.product.as_ref().unwrap();
                
                verify_with_msg(&bot, &msg, warehouse)
                    .with(product.clone())
                    .left_at_least(amount)
                    .await?
                    .merchant_is_not(user.0.name.clone())
                    .await?;

                if product.negotiated_price {
                    data.payment_method = Some(PurchaseWith::Negotiated);
                    bot.send_message(
                        msg.chat.id,
                        localize_msg!(warehouse, msg,
                            "Do you really want to buy {amount}x {name} at a negotiated price?",
                            "amount" => amount, 
                            "name" => localize_msg!(warehouse, msg, data.item.as_ref().unwrap().name)
                        ),
                    )
                    .parse_mode(ParseMode::Html)
                    .reply_markup(ReplyMarkup::Keyboard(KeyboardMarkup {
                        resize_keyboard: Some(true),
                        one_time_keyboard: Some(true),
                        keyboard: vec![vec![
                            KeyboardButton::new(localize_msg!(warehouse, msg, "Yes")), 
                            KeyboardButton::new(localize_msg!(warehouse, msg, "No"))]],
                        ..Default::default()
                    }))
                    .await?;
                    return Ok(Self::WaitConfirm(data));
                }

                match product.payment_method {
                    PaymentMethod::Cash => {
                        data.payment_method = Some(PurchaseWith::Cash);
                    }
                    PaymentMethod::Card => {
                        data.payment_method = Some(PurchaseWith::Card);
                    }
                    PaymentMethod::Both => {
                        let text = localize_msg!(warehouse, msg, 
                            "How do you want to pay for the <b>{name}</b>?",
                            "name" => localize_msg!(warehouse, msg, data.item.as_ref().unwrap().name)
                        );

                        bot.send_message(msg.chat.id, text)
                            .parse_mode(ParseMode::Html)
                            .reply_markup(ReplyMarkup::Keyboard(KeyboardMarkup {
                                resize_keyboard: Some(true),
                                one_time_keyboard: Some(true),
                                keyboard: vec![
                                    vec![
                                        KeyboardButton::new(localize_msg!(warehouse, msg, "Card")), 
                                        KeyboardButton::new(localize_msg!(warehouse, msg, "Cash"))],
                                    vec![
                                        KeyboardButton::new(localize_msg!(warehouse, msg, "Cancel"))],
                                ],
                                ..Default::default()
                            }))
                            .await?;

                        return Ok(Self::WaitPaymentMethod(data));
                    }
                };

                Ok(Self::WaitConfirm(data))
            }
            _ => Ok(self),
        }
    }
}

#[async_trait]
impl ConversationStage<String> for Stage {
    async fn next(
        self,
        bot: Bot,
        msg: Message,
        user: (User, UserMeta),
        warehouse: &mut Warehouse,
        text: String,
    ) -> Result<Self> {
        match self {
            Stage::WaitPaymentMethod(mut data) => {
                let cash_txt = localize_msg!(warehouse, msg, "Cash").to_lowercase();
                let card_txt = localize_msg!(warehouse, msg, "Card").to_lowercase();

                match text.to_lowercase().as_str() {
                    txt if txt == cash_txt => {
                        data.payment_method = Some(PurchaseWith::Cash);
                    }
                    txt if txt == card_txt => {
                        data.payment_method = Some(PurchaseWith::Card);
                    }
                    _ => {
                        let text = localize_msg!(warehouse, msg,
                            "Sorry, but I don't understand what you mean by {item}, please try again.",
                            "item" => text
                        );

                        bot.send_message(msg.chat.id, text)
                            .parse_mode(ParseMode::Html)
                            .await?;

                        return Ok(Self::WaitPaymentMethod(data));
                    }
                }

                let product = data.product.as_ref().unwrap();
                let item = data.item.as_ref().unwrap();
                let amount = data.amount.unwrap();

                bot.send_message(
                    msg.chat.id,
                    localize_msg!(warehouse, msg,
                        "Do you really want to buy {amount}x {name} for <b>{price}</b>?",
                        "amount" => amount, 
                        "name" => localize_msg!(warehouse, msg, item.name),
                        "price" => product.currency.format(&(product.price * amount as f64).to_string())
                    ),
                )
                .reply_markup(ReplyMarkup::Keyboard(KeyboardMarkup {
                    resize_keyboard: Some(true),
                    one_time_keyboard: Some(true),
                    keyboard: vec![vec![
                        KeyboardButton::new(localize_msg!(warehouse, msg, "Yes")), 
                        KeyboardButton::new(localize_msg!(warehouse, msg, "No"))]],
                    ..Default::default()
                }))
                .parse_mode(ParseMode::Html)
                .await?;

                Ok(Self::WaitConfirm(data))
            }

            Stage::WaitConfirm(data) => {
                let lang_code = msg
                    .from()
                    .map(|u| u.language_code.clone())
                    .flatten()
                    .unwrap_or("en".to_owned());

                let yes_text = localize_msg!(warehouse, msg, "Yes").to_lowercase();

                if text.to_lowercase() == yes_text {
                    bot.send_message(msg.chat.id, localize_msg!(warehouse, msg, "Preparing order..."))
                        .reply_markup(user_keyboard(warehouse, &lang_code, &user.0).await)
                        .parse_mode(ParseMode::Html)
                        .await?;

                    match data.payment_method.clone().unwrap() {
                        PurchaseWith::Cash => {
                            purchase_by_cash(bot, msg, warehouse, &user.0, data).await?
                        }
                        PurchaseWith::Card => {
                            purchase_by_card(bot, msg, warehouse, &user.0, data).await?
                        }
                        PurchaseWith::Negotiated => {
                            purchase_negitiated(bot, msg, warehouse, &user.0, data).await?
                        }
                    }
                }
                else {
                    bot.send_message(msg.chat.id, localize_msg!(warehouse, msg, "Ok, I've canceled your order."))
                        .reply_markup(user_keyboard(warehouse, &lang_code, &user.0).await)
                        .parse_mode(ParseMode::Html)
                        .await?;
                }

                Ok(Self::Start)
            }
            _ => Ok(self),
        }
    }
}

async fn purchase_negitiated(
    bot: Bot,
    msg: Message,
    warehouse: &mut Warehouse,
    user: &User,
    data: StageData,
) -> Result<()> {
    let product = data.product.unwrap();
    let amount = data.amount.unwrap();

    let order = submit_order(
        &bot,
        &msg,
        warehouse,
        user,
        product,
        amount,
        OrderStage::Negotiated,
    )
    .await?;
    notify_merchant_about_new_order(&bot, &msg, warehouse, user, &order).await?;
    notify_customer_about_order(&bot, &msg, warehouse, user, &order).await?;

    Ok(())
}

async fn purchase_by_cash(
    bot: Bot,
    msg: Message,
    warehouse: &mut Warehouse,
    user: &User,
    data: StageData,
) -> Result<()> {
    let product = data.product.unwrap();
    let amount = data.amount.unwrap();

    let order = submit_order(
        &bot,
        &msg,
        warehouse,
        user,
        product,
        amount,
        OrderStage::WaitForPayment,
    )
    .await?;
    notify_merchant_about_new_order(&bot, &msg, warehouse, user, &order).await?;
    notify_customer_about_order(&bot, &msg, warehouse, user, &order).await?;

    Ok(())
}

async fn purchase_by_card(
    bot: Bot,
    msg: Message,
    warehouse: &mut Warehouse,
    user: &User,
    data: StageData,
) -> Result<()> {
    let product = data.product.unwrap();
    let amount = data.amount.unwrap();

    let order = submit_order(
        &bot,
        &msg,
        warehouse,
        user,
        product,
        amount,
        OrderStage::WaitForPayment,
    )
    .await?;

    notify_merchant_about_new_order(&bot, &msg, warehouse, user, &order).await?;
    notify_customer_about_order(&bot, &msg, warehouse, user, &order).await?;

    let chat_lang_code = msg.from()
        .map(|u| u.language_code
            .as_ref()
            .map(|c| c
                .as_str()))
        .flatten()
        .unwrap_or("en");

    invoice::send_invoice(bot, msg.chat.id, &chat_lang_code, warehouse, user, order).await?;

    Ok(())
}

async fn submit_order(
    bot: &Bot,
    msg: &Message,
    warehouse: &mut Warehouse,
    _: &User,
    mut product: Row<Product>,
    amount: u32,
    stage: OrderStage,
) -> Result<Order> {
    let order = make_order(
        msg.from().unwrap().username.clone().unwrap(),
        stage,
        &product,
        amount,
    );

    let mut customer = verify_with_msg(bot, msg, warehouse)
        .user_by_name(&order.customer)
        .await?
        .into_result();

    let mut customer_meta = verify_with_msg(bot, msg, warehouse)
        .user_meta_by_name(&order.customer)
        .await?
        .into_result();

    let mut merchant_meta = verify_with_msg(bot, msg, warehouse)
        .user_meta_by_name(&order.merchant)
        .await?
        .into_result();

    customer.last_activity_date = Utc::now();
    warehouse.users.update_one(customer.row, &customer).await?;

    warehouse.orders.extend_one(&order).await?;

    customer_meta.pending_orders.push(order.id.clone());
    warehouse
        .users_meta
        .update_one(customer_meta.row, &customer_meta)
        .await?;

    merchant_meta.pending_orders.push(order.id.clone());
    warehouse
        .users_meta
        .update_one(merchant_meta.row, &merchant_meta)
        .await?;

    product.amount_left -= amount;
    warehouse.products.update_one(product.row, &product).await?;

    update_user_activity(warehouse, &order.customer).await?;

    Ok(order)
}

fn make_order(customer: String, stage: OrderStage, product: &Product, amount: u32) -> Order {
    Order {
        id: minimal_id::Generator::new_id().to_string(),
        customer: customer,
        merchant: product.merchant.clone(),
        stage,
        item_id: product.item_id.clone(),
        amount,
        cost: if product.negotiated_price {
            0f64
        } else {
            product.price * amount as f64
        },
        currency: product.currency,
        date: Utc::now(),
    }
}

async fn notify_merchant_about_new_order(
    bot: &Bot,
    msg: &Message,
    warehouse: &mut Warehouse,
    _: &User,
    order: &Order,
) -> Result<()> {
    let merchant_chat_id = verify_with_msg(&bot, msg, warehouse)
        .user_meta_by_name(&order.merchant)
        .await?
        .has_chat_id()
        .await?
        .into_result()
        .chat_id
        .unwrap();

    let item = verify_with_msg(&bot, msg, warehouse)
        .item_by_id(&order.item_id)
        .await?
        .into_result();

    bot.send_message(
        merchant_chat_id,
        localize_msg!(warehouse, msg,
            "You have a new order for a {name} item from @{customer}!",
            "name" => localize_msg!(warehouse, msg, item.name), 
            "customer" => order.customer
        ),
    )
    .reply_markup(ReplyMarkup::inline_kb(vec![vec![
        InlineKeyboardButton::switch_inline_query_current_chat(
            localize_msg!(warehouse, msg, "Details"),
            format!(".o {}", order.id),
        ),
    ]]))
    .await?;

    Ok(())
}

async fn notify_customer_about_order(
    bot: &Bot,
    msg: &Message,
    warehouse: &mut Warehouse,
    _: &User,
    order: &Order,
) -> Result<()> {
    bot.send_message(
        msg.chat.id,
        localize_msg!(warehouse, msg,
            concat!(
                "Thank you for your order! The seller @{merchant} will be in touch ",
                "with you soon, but if you have any questions, you can ask him yourself."),
            "merchant" => order.merchant
        )
        .as_str(),
    )
    .parse_mode(ParseMode::Html)
    .reply_markup(ReplyMarkup::inline_kb(vec![vec![
        InlineKeyboardButton::switch_inline_query_current_chat(
            localize_msg!(warehouse, msg, "Details"),
            format!(".o {}", order.id),
        ),
    ]]))
    .await?;

    Ok(())
}
