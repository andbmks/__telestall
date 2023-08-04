use async_trait::async_trait;
use chrono::Utc;
use teloxide::{
    dispatching::dialogue::InMemStorage,
    prelude::*,
    types::{KeyboardButton, KeyboardMarkup, ParseMode, ReplyMarkup, Update, UpdateKind},
};

use crate::utils::verify::prelude::*;
use crate::utils::{payload::PayloadOp, verify::verify_with_msg};
use crate::{dialogues::enter_user_dialogue, prelude::*, utils::row::Row};

type Storage = InMemStorage<Stage>;

#[derive(Default, Clone)]
struct StageData {
    pub product: Option<Row<Product>>,
    pub amount: Option<u32>,
}

#[derive(Default, Clone)]
enum Stage {
    #[default]
    Start,
    WaitAmount(StageData),
    WaitConfirm(StageData),
}

pub fn handler() -> HandlerResult {
    dptree::entry()
        // Handle purcahse
        .branch(
            Update::filter_callback_query()
                .chain(enter_user_dialogue::<Storage, Stage>(
                    "To purchase a product you first need to start a dialog with the bot.",
                ))
                .filter(callback_prefix(PayloadOp::Redeem))
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
                    dptree::case![Stage::WaitConfirm(data)]
                        .endpoint(receive_text_stage::<Stage, Storage>),
                ),
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
                let chat_id = user.1.chat_id.unwrap();
                let payload_content = q.data.clone().unwrap_or("".to_owned());

                let product = verify_with_callback(&bot, &q, warehouse)
                    .payload_str(&payload_content)
                    .await?
                    .verify_product()
                    .await?
                    .visible_to_user(&user.0)
                    .await?
                    .left_at_least(1)
                    .await?
                    .price_is_not_negotiated()
                    .await?
                    .merchant_is(user.0.name)
                    .await?
                    .into_result();

                let item = verify_with_callback(&bot, &q, warehouse)
                    .item_by_id(&product.item_id)
                    .await?
                    .into_result();

                bot.send_message(chat_id,
                    localize_upd!(warehouse, upd,
                        "You wanted to redeem the <b>{item}</b>, I'm very pleased! Just need to clarify how much you want to redeem?",
                        "item" => item.name
                    ))
                    .parse_mode(ParseMode::Html)
                    .reply_markup(ReplyMarkup::Keyboard(KeyboardMarkup {
                        resize_keyboard: Some(true),
                        one_time_keyboard: Some(true),
                        keyboard: vec![
                            (1..=5).map(|x| KeyboardButton::new(x.to_string())).collect(),
                            vec![KeyboardButton::new(
                                localize_upd!(warehouse, upd, "Cancel"))],
                        ],
                        ..Default::default()
                    }))
                    .await?;

                bot.answer_callback_query(q.id.clone()).await?;

                Ok(Self::WaitAmount(StageData {
                    product: Some(product),
                    amount: None,
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
                        localize_msg!(warehouse, msg, "You can't redeem more than you have."),
                    )
                    .await?;
                    return Ok(Self::WaitAmount(data));
                }

                let item = verify_with_msg(&bot, &msg, warehouse)
                    .item_by_id(&product.item_id)
                    .await?
                    .into_result();

                let price = product
                    .currency
                    .format(&(product.price * amount as f64).to_string());

                bot.send_message(
                    msg.chat.id,
                    localize_msg!(
                        warehouse,
                        msg,
                        "Do you really want to redeem {amount}x {name} for {price}?",
                        "amount" => amount,
                        "name" => item.name,
                        "price" => price
                    ),
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
            Stage::WaitConfirm(StageData {
                product: Some(product),
                amount: Some(amount),
                ..
            }) => {
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

                let product = verify_with_msg(&bot, &msg, warehouse)
                    .with(product.clone())
                    .visible_to_user(&user.0)
                    .await?
                    .left_at_least(amount)
                    .await?
                    .price_is_not_negotiated()
                    .await?
                    .merchant_is(&user.0.name)
                    .await?
                    .update(|p| {
                        p.amount_left -= amount;
                        p.amount_sold += amount;
                    })
                    .await?
                    .into_result();

                let sale = Sale {
                    merchant: user.0.name.clone(),
                    sale_type: SaleType::Redeem,
                    customer: user.0.name.clone(),
                    item_id: product.item_id.clone(),
                    comment: "Redeemed".to_string(),
                    amount,
                    revenue: product.price * amount as f64,
                    currency: product.currency.clone(),
                    share: 0f32,
                    date: Utc::now(),
                };

                verify_with_msg(&bot, &msg, warehouse)
                    .with(Row::new(0, sale))
                    .publish()
                    .await?;

                bot.send_message(
                    msg.chat.id,
                    localize_msg!(warehouse, msg, "The product was successfully redeemed!"),
                )
                .reply_markup(user_keyboard(warehouse, &lang_code, &user.0).await)
                .await?;

                update_user_activity(warehouse, &user.0.name).await?;

                Ok(Self::Start)
            }
            _ => Ok(Self::Start),
        }
    }
}
