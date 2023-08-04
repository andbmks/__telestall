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

use crate::dialogues::stages::verify_product;
use crate::prelude::*;

type Storage = InMemStorage<Stage>;

#[derive(Default, Clone)]
struct StageData {
    pub product: Option<Product>,
    pub item: Option<Item>,
    pub amount: Option<u32>,
    pub cost_price: Option<f64>,
    pub currency: Option<Currency>,
}

#[derive(Default, Clone)]
enum Stage {
    #[default]
    Start,
    WaitProduct,
    WaitAmount(StageData),
    WaitCostPrice(StageData),
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
                .chain(filter_msg_prefix("🪫 Replenish"))
                .endpoint(start::<Stage, Storage>),
        )
        .branch(dptree::case![Stage::WaitProduct].endpoint(receive_product_stage::<Stage, Storage>))
        .branch(
            dptree::case![Stage::WaitAmount(data)].endpoint(receive_amount_stage::<Stage, Storage>),
        )
        .branch(
            dptree::case![Stage::WaitCostPrice(data)]
                .endpoint(receive_money_stage::<Stage, Storage>),
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
        Role::Moderator
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
                        "<b>Replenishment</b>\n",
                        "Please select the product you wish to replenish.",
                    )
                );

                let chat_id = upd.chat_id().ok_or(UnkError::unknown("upd.chat_id"))?;

                bot.send_message(chat_id, text)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(InlineKeyboardMarkup::default().append_row(vec![
                        InlineKeyboardButton::switch_inline_query_current_chat(
                            localize_upd!(warehouse, upd, "Select"),
                            "~repl ",
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
                bot.send_message(
                    msg.chat.id,
                    localize_msg!(
                        warehouse,
                        msg,
                        "Great! Please tell me how much you want to replenish."
                    ),
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
                bot.send_message(msg.chat.id, localize_msg!(warehouse, msg, concat!(
                    "Please tell me the total cost of all items. ",
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
                Ok(Self::WaitCostPrice(data))
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
        user: (User, UserMeta),
        warehouse: &mut Warehouse,
        money: (f64, Currency),
    ) -> Result<Self> {
        match self {
            Stage::WaitCostPrice(mut data) => {
                let price = money.1.format(&money.0.to_string());

                let text = [
                    "<b>Confirm the sell</b>".to_owned(),
                    localize_msg!(warehouse, msg, "• Product: {product}", "product" => data.item.as_ref().unwrap().name),
                    localize_msg!(warehouse, msg, "• Amount: {amount}", "amount" => data.amount.as_ref().unwrap()),
                    localize_msg!(warehouse, msg, "• Price: {price}", "price" => price),
                    localize_msg!(warehouse, msg, "• Supplier: {supplier}", "supplier" => user.0.name),
                    localize_msg!(warehouse, msg, "• Merchant: {merchant}", "merchant" => data.product.as_ref().unwrap().merchant),
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

                data.cost_price = Some(money.0);
                data.currency = Some(money.1);

                Ok(Self::WaitConfirmation(data))
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
                        return Ok(Self::Start);
                    }
                };

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
                                localize_msg!(
                                    warehouse, msg,
                                "Product was edited during the dialogue, so you can't sell that much."),
                            )
                            .reply_markup(user_keyboard(warehouse, &lang_code, &user.0).await)
                            .await?;
                        return Ok(Self::Start);
                    }
                };

                product.amount_granted += data.amount.unwrap();
                product.amount_left += data.amount.unwrap();

                match warehouse.products.update_one(row, &product).await {
                    Ok(_) => (),
                    Err(e) => {
                        bot.send_message(
                            msg.chat.id,
                            localize_msg!(warehouse, msg, "Failed to update the product state."),
                        )
                        .reply_markup(user_keyboard(warehouse, &lang_code, &user.0).await)
                        .await?;
                        return Err(Box::new(e));
                    }
                }

                bot.send_message(msg.chat.id, localize_msg!(warehouse, msg, "Processing..."))
                    .reply_markup(user_keyboard(warehouse, &lang_code, &user.0).await)
                    .await?;

                let replenishment = Replenishment {
                    supplier: user.0.name.clone(),
                    merchant: product.merchant.clone(),
                    item_id: product.item_id.clone(),
                    amount: data.amount.unwrap(),
                    cost_price: data.cost_price.unwrap(),
                    currency: data.currency.unwrap(),
                    date: Utc::now(),
                };

                match warehouse.replenishments.extend_one(&replenishment).await {
                    Ok(_) => {
                        bot.send_message(
                            msg.chat.id,
                            localize_msg!(
                                warehouse,
                                msg,
                                "The replenishment was successfully registered."
                            ),
                        )
                        .reply_markup(user_keyboard(warehouse, &lang_code, &user.0).await)
                        .await?;
                    }
                    Err(e) => {
                        bot.send_message(
                            msg.chat.id,
                            localize_msg!(warehouse, msg, "Failed to register the replenishment."),
                        )
                        .reply_markup(user_keyboard(warehouse, &lang_code, &user.0).await)
                        .await?;

                        // Rust analyzer goes wild when I try to use return here
                        Err(Box::new(e))?;
                    }
                }

                update_user_activity(warehouse, &user.0.name).await?;

                Ok(Self::Start)
            }
            _ => Ok(Self::Start),
        }
    }
}
