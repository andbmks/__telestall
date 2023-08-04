use async_trait::async_trait;
use teloxide::{
    dispatching::dialogue::InMemStorage,
    prelude::*,
    types::{
        InlineKeyboardButton, KeyboardButton, KeyboardMarkup, ReplyMarkup, Update, UpdateKind,
    },
};

use crate::{
    dialogues::enter_user_dialogue,
    prelude::*,
    utils::{
        payload::PayloadOp,
        row::Row,
        verify::{verify_with_callback, verify_with_msg},
    },
};

type Storage = InMemStorage<Stage>;

#[derive(Default, Clone)]
struct StageData {
    pub order: Option<Row<Order>>,
}

#[derive(Default, Clone)]
enum Stage {
    #[default]
    Start,
    WaitPrice(StageData),
}

pub fn handler() -> HandlerResult {
    dptree::entry()
        .branch(
            Update::filter_callback_query()
                .chain(enter_user_dialogue::<Storage, Stage>(
                    "To purchase a product you first need to start a dialog with the bot.",
                ))
                .filter(callback_prefix(PayloadOp::SpecifyOrderPrice))
                .endpoint(start::<Stage, Storage>),
        )
        .branch(
            Update::filter_message()
                .enter_dialogue::<Message, Storage, Stage>()
                .branch(
                    filter_dialogue_started::<Stage, Storage>()
                        .chain(filter_msg_prefix("Cancel"))
                        .endpoint(cancel::<Stage, Storage>),
                )
                .branch(
                    dptree::case![Stage::WaitPrice(data)]
                        .endpoint(receive_money_stage::<Stage, Storage>),
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
            Stage::Start => false,
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
        let chat_id = user.1.chat_id.ok_or(UnkError::unknown("No chat id"))?;

        let UpdateKind::CallbackQuery(q) = upd.kind.clone() else {
            Err(UnkError::unknown("Invalid update kind"))?
        };

        let order = verify_with_callback(&bot, &q, warehouse)
            .payload_str_opt(&q.data)
            .await?
            .verify_order()
            .await?
            .stage_is(OrderStage::Negotiated)
            .await?
            .into_result();

        bot.answer_callback_query(q.id).await?;
        bot.send_message(
            chat_id,
            localize_upd!(warehouse, upd,
                concat!(
                    "Please enter the price for all items in total. ",
                    "Write as a real number with a currency (for example, \"100.50 eur\" or \"30 CZK\").")
                ),
        )
        .reply_markup(ReplyMarkup::Keyboard(KeyboardMarkup {
            resize_keyboard: Some(true),
            one_time_keyboard: Some(true),
            keyboard: vec![vec![KeyboardButton::new(localize_upd!(warehouse, upd, "Cancel"))]],
            ..Default::default()
        }))
        .await?;

        Ok(Self::WaitPrice(StageData { order: Some(order) }))
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
            Stage::WaitPrice(StageData {
                order: Some(order), ..
            }) => {
                let lang_code = msg
                    .from()
                    .map(|u| u.language_code.clone())
                    .flatten()
                    .unwrap_or("en".to_owned());

                bot.send_message(msg.chat.id, localize_msg!(warehouse, msg, "Processing..."))
                    .reply_markup(user_keyboard(warehouse, &lang_code, &user.0).await)
                    .await?;

                warehouse.users_meta.refresh().await?;

                let customer_chat_id = verify_with_msg(&bot, &msg, warehouse)
                    .user_meta_by_name(&order.customer)
                    .await?
                    .has_chat_id()
                    .await?
                    .into_result()
                    .chat_id
                    .unwrap();

                let item = verify_with_msg(&bot, &msg, warehouse)
                    .item_by_id(&order.item_id)
                    .await?
                    .into_result();

                let order = verify_with_msg(&bot, &msg, warehouse)
                    .with(order)
                    .update(|o| {
                        o.cost = money.0;
                        o.currency = money.1;
                        o.stage = OrderStage::WaitForPayment;
                    })
                    .await?
                    .into_result();

                bot.send_message(
                    msg.chat.id,
                    localize_msg!(warehouse, msg, "You have successfully priced your order!"),
                )
                .reply_markup(ReplyMarkup::inline_kb(vec![vec![
                    InlineKeyboardButton::switch_inline_query_current_chat(
                        localize_msg!(warehouse, msg, "Details"),
                        format!(".o {}", order.id),
                    ),
                ]]))
                .await?;

                bot.send_message(
                    customer_chat_id,
                    localize_msg!(
                        warehouse,
                        msg,
                        "The seller has priced the {name}, you can now pay for it!",
                        "name" => item.name
                    ),
                )
                .reply_markup(ReplyMarkup::inline_kb(vec![vec![
                    InlineKeyboardButton::switch_inline_query_current_chat(
                        localize_msg!(warehouse, msg, "Details"),
                        format!(".o {}", order.id),
                    ),
                ]]))
                .await?;

                Ok(Self::Start)
            }
            _ => Ok(self),
        }
    }
}
