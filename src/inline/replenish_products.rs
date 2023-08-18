use teloxide::prelude::*;
use teloxide::types::{
    InlineQueryResult, InlineQueryResultArticle, InputMessageContent, InputMessageContentText,
    ParseMode,
};

use crate::entries::search_group;
use crate::prelude::*;

use super::InlineRequest;

impl<'a> InlineRequest<'a> {
    pub async fn make_replenish(&mut self) -> Result<()> {
        let pairs: Vec<_> = self
            .warehouse
            .products
            .inner
            .read()?
            // Map item to the iterator
            .filter_map(|product| {
                self.warehouse
                    .items
                    .by_id
                    .get_with_row(&product.item_id)
                    .map(|(_, item)| (product.clone(), item.clone()))
            })
            // Filter by query
            .filter(|(product, item)| {
                self.warehouse
                    .items
                    .search
                    .get(&item.id)
                    .unwrap()
                    .search_all(search_group::MERCHANT, self.query.iter())
                    || self
                        .warehouse
                        .products
                        .search
                        .get(&product.id())
                        .unwrap()
                        .search_all(search_group::MERCHANT, self.query.iter())
            })
            .skip(self.page * 49)
            .take(49)
            .collect();

        let mut results = vec![];

        for (product, item) in pairs {
            results.push(InlineQueryResult::Article(
                self.make_repl_article(&product, &item).await?,
            ));
        }

        self.process_results(&mut results).await;

        self.bot
            .answer_inline_query(&self.q.id, results)
            .cache_time(0)
            .await?;

        Ok(())
    }

    async fn make_repl_article(
        &mut self,
        product: &Product,
        item: &Item,
    ) -> Result<InlineQueryResultArticle> {
        let content = make_product_answer(product, item);

        let content = InputMessageContent::Text(
            InputMessageContentText::new(content).parse_mode(ParseMode::Html),
        );

        Ok(InlineQueryResultArticle::new(
            format!("p?{}", product.id()),
            item.name.to_owned(),
            content,
        )
        .description(self.make_repl_description(product).await)
        .hide_url(true)
        .thumb_url(item.image_url.clone().parse().unwrap()))
    }

    async fn make_repl_description(&mut self, product: &Product) -> String {
        let mut line0 = vec![];
        let mut line1 = vec![];

        // Add merchant
        line0.push(localize!(
            self.warehouse,
            &self.lang_code,
            "by @{merchant}",
            "merchant" => product.merchant
        ));

        // Add price
        let price = if product.negotiated_price {
            localize!(self.warehouse, &self.lang_code, "Negotiated")
        } else {
            product.currency.format(&product.price.to_string())
        };

        match product.payment_method {
            PaymentMethod::Cash => line0.push(format!("{} 💵", price)),
            PaymentMethod::Card => line0.push(format!("{} 💳", price)),
            PaymentMethod::Both => line0.push(format!("{}", price)),
        };

        // Add amount granted
        line1.push(localize!(
            self.warehouse,
            &self.lang_code,
            "{amount} granted",
            "amount" => product.amount_granted
        ));

        // Add amount sold
        line1.push(localize!(
            self.warehouse,
            &self.lang_code,
            "{amount} sold",
            "amount" => product.amount_sold
        ));

        // Add amount left
        line1.push(localize!(
            self.warehouse,
            &self.lang_code,
            "{amount} left",
            "amount" => product.amount_left
        ));

        format!("{}\n{}", line0.join(" • "), line1.join(" • "))
    }
}
