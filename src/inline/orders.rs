use tables::prelude::*;
use teloxide::prelude::*;
use teloxide::types::{
    InlineKeyboardButton, InlineKeyboardMarkup, InlineQueryResult, InlineQueryResultArticle,
    InputMessageContent, InputMessageContentText, ParseMode,
};

use crate::entries::search_group;
use crate::prelude::*;
use crate::utils::payload::Payload;

use super::InlineRequest;

impl<'a> InlineRequest<'a> {
    pub async fn make_orders(&mut self) -> Result<()> {
        self.warehouse.orders.refresh().await?;
        self.warehouse.products.refresh().await?;
        self.warehouse.items.refresh().await?;
        self.warehouse.users_meta.refresh().await?;

        let user_meta = self
            .warehouse
            .users_meta
            .by_name
            .get(&self.user.name)
            .ok_or(UnkError::unknown("No user meta"))?
            .clone();

        let pairs: Vec<_> = user_meta
            .pending_orders
            .iter()
            .chain(user_meta.completed_orders.iter())
            .filter_map(|order_id| self.warehouse.orders.by_id.get(order_id).cloned())
            // Filter by query
            .filter(|order| {
                let mut pass = self
                    .warehouse
                    .items
                    .search
                    .get(&order.item_id)
                    .unwrap()
                    .search(search_group::USER, self.query.iter());

                pass |= self
                    .warehouse
                    .orders
                    .search
                    .get(&order.id)
                    .unwrap()
                    .search(search_group::USER, self.query.iter());

                pass
            })
            .filter_map(|order| {
                self.warehouse
                    .products
                    .by_id
                    .get(&order.product_id())
                    .map(|product| (order, product.clone()))
            })
            .collect();

        let mut results = vec![];

        for (order, product) in pairs {
            let item = self
                .warehouse
                .items
                .by_id
                .get(&order.item_id)
                .unwrap()
                .clone();

            results.push(InlineQueryResult::Article(
                self.make_article(&order, &product, &item).await?,
            ));
        }

        results.truncate(50);

        self.bot
            .answer_inline_query(&self.q.id, results)
            .cache_time(0)
            .await?;

        Ok(())
    }

    async fn make_article(
        &mut self,
        order: &Order,
        product: &Product,
        item: &Item,
    ) -> Result<InlineQueryResultArticle> {
        Ok(InlineQueryResultArticle::new(
            format!("o?{}", order.id),
            localize!(self.warehouse, &self.lang_code, item.name.to_owned()),
            self.make_content(order, item).await,
        )
        .description(self.make_description(order, item).await)
        .reply_markup(self.make_markup(order, product).await)
        .hide_url(true)
        .thumb_url(item.image_url.clone().parse().unwrap()))
    }

    async fn make_content(&mut self, order: &Order, item: &Item) -> InputMessageContent {
        let paid = match order.stage {
            OrderStage::Negotiated | OrderStage::WaitForPayment | OrderStage::Cancelled => {
                "-".to_owned()
            }
            _ => order.currency.format(&order.cost.to_string()),
        };

        let text = [
            localize!(self.warehouse, &self.lang_code, "• Product: {product}", "product" => item.name),
            localize!(self.warehouse, &self.lang_code, "• Merchant: {merchant}", "merchant" => order.merchant),
            localize!(self.warehouse, &self.lang_code, "• Customer: {customer}", "customer" => order.customer),
            localize!(self.warehouse, &self.lang_code, "• Stage: {stage}", "stage" => format!("{:?}", order.stage)),
            localize!(self.warehouse, &self.lang_code, "• Amount: {amount}", "amount" => order.amount),
            localize!(self.warehouse, &self.lang_code, "• Paid: {paid}", "paid" => paid),
            localize!(self.warehouse, &self.lang_code, "• Date: {date}", "date" => order.date),
            localize!(self.warehouse, &self.lang_code, "• Id: {id}", "id" => order.id),
        ]
        .join("\n");

        InputMessageContent::Text(InputMessageContentText::new(text).parse_mode(ParseMode::Html))
    }

    async fn make_description(&mut self, order: &Order, item: &Item) -> String {
        // Assign with an inline description
        let description = localize!(self.warehouse, &self.lang_code, item.inline_desc.clone());
        let mut info = vec![];

        // Add merchant
        if order.merchant == self.user.name {
            info.push(localize!(self.warehouse, &self.lang_code, "Incoming").to_string());
        }

        // Add stage
        match order.stage {
            OrderStage::Negotiated => {
                info.push(localize!(self.warehouse, &self.lang_code, "💬 Negotiated").to_string())
            }
            OrderStage::WaitForPayment => {
                info.push(localize!(self.warehouse, &self.lang_code, "💳 Waiting").to_string())
            }
            OrderStage::Paid => {
                info.push(localize!(self.warehouse, &self.lang_code, "💰 Paid").to_string())
            }
            OrderStage::Completed => {
                info.push(localize!(self.warehouse, &self.lang_code, "✅ Completed").to_string())
            }
            OrderStage::Cancelled => {
                info.push(localize!(self.warehouse, &self.lang_code, "❌ Cancelled").to_string())
            }
        }

        // Add amount
        info.push(localize!(self.warehouse, &self.lang_code,
            "{amount} piece{end}",
            "amount" => order.amount,
            "end" => if order.amount > 1 { "s" } else { "" }
        ));

        format!("{}\n{}", description, info.join(" • "))
    }

    async fn make_markup(&mut self, order: &Order, product: &Product) -> InlineKeyboardMarkup {
        let mut markup = InlineKeyboardMarkup::default();

        if self.user.name == order.merchant {
            markup = match order.stage {
                OrderStage::Negotiated => markup.append_row(vec![
                    InlineKeyboardButton::callback(
                        localize!(self.warehouse, &self.lang_code, "Cancel"),
                        Payload::cancel_order(order.id.clone()).to_string(),
                    ),
                    InlineKeyboardButton::callback(
                        localize!(self.warehouse, &self.lang_code, "Specify price"),
                        Payload::specify_order_price(order.id.clone()).to_string(),
                    ),
                ]),
                OrderStage::WaitForPayment => markup.append_row(vec![
                    InlineKeyboardButton::callback(
                        localize!(self.warehouse, &self.lang_code, "Cancel"),
                        Payload::cancel_order(order.id.clone()).to_string(),
                    ),
                    InlineKeyboardButton::callback(
                        localize!(self.warehouse, &self.lang_code, "Complete"),
                        Payload::complete_order(order.id.clone()).to_string(),
                    ),
                ]),
                OrderStage::Paid => markup.append_row(vec![InlineKeyboardButton::callback(
                    localize!(self.warehouse, &self.lang_code, "Complete"),
                    Payload::complete_order(order.id.clone()).to_string(),
                )]),
                _ => markup,
            };
        } else if self.user.name == order.customer {
            markup = match order.stage {
                OrderStage::Negotiated => markup.append_row(vec![InlineKeyboardButton::callback(
                    localize!(self.warehouse, &self.lang_code, "Cancel"),
                    Payload::cancel_order(order.id.clone()).to_string(),
                )]),
                OrderStage::WaitForPayment => {
                    let mut buttons = vec![];

                    if product.payment_method.supports_card() {
                        buttons.push(InlineKeyboardButton::callback(
                            localize!(self.warehouse, &self.lang_code, "Pay with card"),
                            Payload::pay_for_order(order.id.clone()).to_string(),
                        ));
                    }

                    buttons.push(InlineKeyboardButton::callback(
                        localize!(self.warehouse, &self.lang_code, "Cancel"),
                        Payload::cancel_order(order.id.clone()).to_string(),
                    ));

                    markup.append_row(buttons)
                }
                _ => markup,
            };
        }

        markup
    }
}
