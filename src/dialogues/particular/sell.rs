use async_trait::async_trait;
use chrono::Utc;
use teloxide::{
    dispatching::dialogue::{GetChatId, InMemStorage},
    prelude::*,
    types::{
        Currency, InlineKeyboardButton, InlineKeyboardMarkup, KeyboardButton, KeyboardMarkup,
        ParseMode, ReplyMarkup, Update,
    },
};

use crate::{dialogues::stages::verify_product, prelude::*};

type Storage = InMemStorage<Stage>;

#[derive(Default, Clone)]
struct StageData {
    pub product: Option<Product>,
    pub item: Option<Item>,
    pub amount: Option<u32>,
    pub revenue: Option<f64>,
    pub currency: Option<Currency>,
    pub customer: Option<String>,
    pub comment: Option<String>,
}

#[derive(Default, Clone)]
enum Stage {
    #[default]
    Start,
    WaitProduct,
    WaitAmount(StageData),
    WaitRevenue(StageData),
    WaitCustomer(StageData),
    WaitComment(StageData),
    WaitConfirmation(StageData),
}

pub fn handler() -> HandlerResult {
    Update::filter_message()
        .enter_dialogue::<Message, InMemStorage<Stage>, Stage>()
        .branch(
            filter_dialogue_started::<Stage, Storage>()
                .chain(filter_msg_prefix("Cancel"))
                .endpoint(cancel::<Stage, Storage>),
        )
        .branch(
            dptree::case![Stage::Start]
                .chain(filter_msg_prefix("💸 Sell"))
                .endpoint(start::<Stage, Storage>),
        )
        .branch(dptree::case![Stage::WaitProduct].endpoint(receive_product_stage::<Stage, Storage>))
        .branch(
            dptree::case![Stage::WaitAmount(data)].endpoint(receive_amount_stage::<Stage, Storage>),
        )
        .branch(
            dptree::case![Stage::WaitRevenue(data)].endpoint(receive_money_stage::<Stage, Storage>),
        )
        .branch(
            dptree::case![Stage::WaitCustomer(data)].endpoint(receive_text_stage::<Stage, Storage>),
        )
        .branch(
            dptree::case![Stage::WaitComment(data)].endpoint(receive_text_stage::<Stage, Storage>),
        )
        .branch(
            dptree::case![Stage::WaitConfirmation(data)]
                .endpoint(receive_text_stage::<Stage, Storage>),
        )
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
        Role::Merchant
    }

    async fn start(
        self,
        bot: Bot,
        upd: Update,
        _: (User, UserMeta),
        warehouse: &mut Warehouse,
    ) -> Result<Self> {
        match self {
            Self::Start => {
                let text = localize_upd!(
                    warehouse,
                    upd,
                    concat!(
                        "<b>Sell</b>\n",
                        "Please select the product you wish to sell.",
                    )
                );

                let chat_id = upd.chat_id().ok_or(UnkError::unknown("upd.chat_id"))?;
                bot.send_message(chat_id, text)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(InlineKeyboardMarkup::default().append_row(vec![
                        InlineKeyboardButton::switch_inline_query_current_chat(
                            localize_upd!(warehouse, upd, "Select").to_string(),
                            "~sell ",
                        ),
                    ]))
                    .send()
                    .await?;

                bot.send_message(
                    chat_id,
                    localize_upd!(
                        warehouse,
                        upd,
                        "You can terminate dialogue at any time by pressing Cancel."
                    ),
                )
                .reply_markup(ReplyMarkup::Keyboard(KeyboardMarkup {
                    resize_keyboard: Some(true),
                    is_persistent: true,
                    keyboard: vec![vec![KeyboardButton::new(localize_upd!(
                        warehouse, upd, "Cancel"
                    ))]],
                    ..Default::default()
                }))
                .await?;
                Ok(Self::WaitProduct)
            }
            _ => Ok(self),
        }
    }
}

#[async_trait]
impl ConversationStage<(Product, Item)> for Stage {
    async fn next(
        self,
        bot: Bot,
        msg: Message,
        _: (User, UserMeta),
        warehouse: &mut Warehouse,
        pair: (Product, Item),
    ) -> Result<Self> {
        match self {
            Stage::WaitProduct => {
                if pair.0.amount_left <= 0 {
                    bot.send_message(
                        msg.chat.id,
                        localize_msg!(warehouse, msg,
                            "This product is out of stock, please choose another one or terminate the dialogue."),
                    )
                    .await?;
                    return Ok(self);
                }

                bot.send_message(
                    msg.chat.id,
                    localize_msg!(warehouse, msg, "Great! Please tell me how much you sold."),
                )
                .reply_markup(ReplyMarkup::Keyboard(KeyboardMarkup {
                    resize_keyboard: Some(true),
                    is_persistent: true,
                    keyboard: vec![
                        (1..=5)
                            .map(|i| KeyboardButton::new(i.to_string()))
                            .collect(),
                        vec![KeyboardButton::new(localize_msg!(warehouse, msg, "Cancel"))],
                    ],
                    ..Default::default()
                }))
                .await?;

                Ok(Self::WaitAmount(StageData {
                    product: Some(pair.0),
                    item: Some(pair.1),
                    ..Default::default()
                }))
            }
            _ => Ok(self),
        }
    }
}

#[async_trait]
impl ConversationStage<u32> for Stage {
    async fn next(
        self,
        bot: Bot,
        msg: Message,
        _: (User, UserMeta),
        warehouse: &mut Warehouse,
        amount: u32,
    ) -> Result<Self> {
        match self {
            Stage::WaitAmount(mut data) => {
                data.amount = Some(amount as u32);
                let product = data.product.as_ref().unwrap();

                if product.amount_left < amount {
                    bot.send_message(
                        msg.chat.id,
                        localize_msg!(warehouse, msg, "You can't sell more than you have."),
                    )
                    .await?;
                    return Ok(Self::WaitAmount(data));
                }

                if product.negotiated_price {
                    bot.send_message(msg.chat.id, localize_msg!(warehouse, msg, concat!(
                        "Fine, how much money did you get for all this in total? ",
                        "Write as a real number with a currency (for example, \"100.50 eur\" or \"30 CZK\").")))
                        .reply_markup(ReplyMarkup::Keyboard(KeyboardMarkup {
                            resize_keyboard: Some(true),
                            is_persistent: true,
                            keyboard: vec![
                                vec![KeyboardButton::new(localize_msg!(warehouse, msg, "Cancel"))],
                            ],
                            ..Default::default()
                        }))
                        .await?;
                    Ok(Self::WaitRevenue(data))
                } else {
                    data.revenue = Some(product.price * amount as f64);
                    data.currency = Some(product.currency);

                    bot.send_message(
                        msg.chat.id,
                        localize_msg!(
                            warehouse,
                            msg,
                            "Fine. Optional: write the buyer's username (like pavel_durov)."
                        ),
                    )
                    .reply_markup(ReplyMarkup::Keyboard(KeyboardMarkup {
                        resize_keyboard: Some(true),
                        keyboard: vec![
                            vec![KeyboardButton::new(localize_msg!(warehouse, msg, "Skip"))],
                            vec![KeyboardButton::new(localize_msg!(warehouse, msg, "Cancel"))],
                        ],
                        ..Default::default()
                    }))
                    .await?;
                    Ok(Self::WaitCustomer(data))
                }
            }
            _ => Ok(self),
        }
    }
}

#[async_trait]
impl ConversationStage<(f64, Currency)> for Stage {
    async fn next(
        self,
        bot: Bot,
        msg: Message,
        _: (User, UserMeta),
        warehouse: &mut Warehouse,
        money: (f64, Currency),
    ) -> Result<Self> {
        match self {
            Stage::WaitRevenue(mut data) => {
                data.revenue = Some(money.0);
                data.currency = Some(money.1);

                bot.send_message(
                    msg.chat.id,
                    localize_msg!(
                        warehouse,
                        msg,
                        "Okay. Optional: write the buyer's username (like pavel_durov)."
                    ),
                )
                .reply_markup(ReplyMarkup::Keyboard(KeyboardMarkup {
                    resize_keyboard: Some(true),
                    keyboard: vec![
                        vec![KeyboardButton::new(localize_msg!(warehouse, msg, "Skip"))],
                        vec![KeyboardButton::new(localize_msg!(warehouse, msg, "Cancel"))],
                    ],
                    ..Default::default()
                }))
                .await?;

                Ok(Self::WaitCustomer(data))
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
            Stage::WaitCustomer(mut data) => {
                let skip_text = localize_msg!(warehouse, msg, "Skip");
                let text = match text.as_str() {
                    text if text == skip_text => "-".to_string(),
                    _ => text,
                }
                .to_owned();

                data.customer = Some(text);

                let text = localize_msg!(
                    warehouse,
                    msg,
                    concat!(
                        "A bit more, optional: write a comment (like \"The product ",
                        "was a little scratched, but the buyer agreed to a discount.\").\n"
                    )
                );
                bot.send_message(msg.chat.id, text).await?;

                Ok(Self::WaitComment(data))
            }
            Stage::WaitComment(mut data) => {
                let skip_text = localize_msg!(warehouse, msg, "Skip");
                let text = match text.as_str() {
                    text if text == skip_text => "-".to_string(),
                    _ => text,
                }
                .to_owned();

                data.comment = Some(text);

                warehouse.items.refresh().await?;

                let price = data
                    .currency
                    .as_ref()
                    .unwrap()
                    .format(&data.revenue.as_ref().unwrap().to_string());

                let text = [
                    "<b>Confirm the sell</b>".to_owned(),
                    localize_msg!(warehouse, msg, "• Product: {product}", "product" => data.item.as_ref().unwrap().name),
                    localize_msg!(warehouse, msg, "• Amount: {amount}", "amount" => data.amount.as_ref().unwrap()),
                    localize_msg!(warehouse, msg, "• Price: {price}", "price" => price),
                    localize_msg!(warehouse, msg, "• Customer: {customer}", "customer" => data.customer.as_ref().unwrap()),
                    localize_msg!(warehouse, msg, "• Comment: {comment}", "comment" => data.comment.as_ref().unwrap()),
                ]
                .join("\n");

                bot.send_message(msg.chat.id, text)
                    .parse_mode(ParseMode::Html)
                    .await?;

                bot.send_message(
                    msg.chat.id,
                    localize_msg!(warehouse, msg, "Is everything correct?"),
                )
                .reply_markup(ReplyMarkup::Keyboard(KeyboardMarkup {
                    resize_keyboard: Some(true),
                    one_time_keyboard: Some(true),
                    keyboard: vec![vec![
                        KeyboardButton::new(localize_msg!(warehouse, msg, "Yes")),
                        KeyboardButton::new(localize_msg!(warehouse, msg, "No")),
                    ]],
                    ..Default::default()
                }))
                .await?;

                Ok(Self::WaitConfirmation(data))
            }
            Stage::WaitConfirmation(data) => {
                let lang_code = msg
                    .from()
                    .map(|u| u.language_code.clone())
                    .flatten()
                    .unwrap_or("en".to_owned());

                let yes_text = localize_msg!(warehouse, msg, "Yes").to_lowercase();

                match text.to_lowercase().as_str() {
                    text if text == yes_text => (),
                    _ => {
                        bot.send_message(
                            msg.chat.id,
                            localize_msg!(warehouse, msg, "Dialogue cancelled."),
                        )
                        .reply_markup(user_keyboard(warehouse, &lang_code, &user.0).await)
                        .await?;
                        return Ok(Stage::Start);
                    }
                };

                bot.send_message(msg.chat.id, localize_msg!(warehouse, msg, "Processing..."))
                    .reply_markup(user_keyboard(warehouse, &lang_code, &user.0).await)
                    .await?;

                let product = data.product.unwrap();

                let (row, mut product) = match verify_product(
                    bot.clone(),
                    &msg,
                    warehouse,
                    &product,
                )
                .await?
                {
                    Some((row, product)) => (*row, product.clone()),
                    None => {
                        bot.send_message(
                            msg.chat.id,
                            localize_msg!(warehouse, msg, "Product was edited during the dialogue, so you can't sell that much."),
                        )
                        .await?;
                        return Ok(Self::Start);
                    }
                };

                if product.amount_left < data.amount.unwrap() {
                    bot.send_message(
                        msg.chat.id,
                        localize_msg!(
                            warehouse,
                            msg,
                            "Product was edited during the dialogue, so you can't sell that much."
                        ),
                    )
                    .await?;
                    return Ok(Self::Start);
                }

                product.amount_left -= data.amount.unwrap();
                product.amount_sold += data.amount.unwrap();

                match warehouse.products.update_one(row, &product).await {
                    Ok(_) => (),
                    Err(e) => {
                        bot.send_message(
                            msg.chat.id,
                            localize_msg!(warehouse, msg, "Failed to update the product state."),
                        )
                        .await?;
                        return Err(Box::new(e));
                    }
                }

                let sale = Sale {
                    merchant: user.0.name.clone(),
                    sale_type: SaleType::HandToHand,
                    customer: data.customer.unwrap(),
                    item_id: product.item_id.clone(),
                    comment: data.comment.unwrap(),
                    amount: data.amount.unwrap(),
                    revenue: data.revenue.unwrap(),
                    currency: data.currency.unwrap(),
                    share: product.share.clone(),
                    date: Utc::now(),
                };

                match warehouse.sales.extend_one(&sale).await {
                    Ok(_) => {
                        bot.send_message(
                            msg.chat.id,
                            localize_msg!(warehouse, msg, "The sale was successfully registered."),
                        )
                        .await?;
                    }
                    Err(e) => {
                        bot.send_message(
                            msg.chat.id,
                            localize_msg!(warehouse, msg, "Failed to register the sale."),
                        )
                        .await?;

                        // Rust analyzer goes wild when I try to use return here
                        Err(Box::new(e))?;
                    }
                }

                update_user_activity(warehouse, &user.0.name).await?;

                Ok(Self::Start)
            }
            _ => Ok(self),
        }
    }
}
