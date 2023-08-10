use tables::prelude::*;
use teloxide::prelude::*;
use teloxide::types::{
    InlineQueryResult, InlineQueryResultArticle, InputMessageContent, InputMessageContentText,
    ParseMode,
};

use crate::entries::search_group;
use crate::prelude::*;

use super::InlineRequest;

impl<'a> InlineRequest<'a> {
    pub async fn make_sells(&mut self) -> Result<()> {
        let pairs: Vec<_> = self
            .warehouse
            .products
            .inner
            .read()?
            // Filter out other merchants' products
            .filter(|product| product.merchant == self.user.name)
            // Map item to iterator
            .filter_map(|product| {
                self.warehouse
                    .items
                    .by_id
                    .get(&product.item_id)
                    .map(|item| (product.clone(), item.clone()))
            })
            // Filter by query
            .filter(|(product, item)| {
                self.warehouse
                    .items
                    .search
                    .get(&item.id)
                    .unwrap()
                    .search_all(&search_group::MERCHANT.to_string(), self.query.iter())
                    || self
                        .warehouse
                        .products
                        .search
                        .get(&product.id())
                        .unwrap()
                        .search_all(&search_group::MERCHANT.to_string(), self.query.iter())
            })
            .skip(self.page * 49)
            .take(49)
            .collect();

        let mut results = vec![];

        for (product, item) in pairs {
            results.push(InlineQueryResult::Article(
                self.make_sell_article(&product, &item).await?,
            ));
        }

        self.process_results(&mut results).await;

        self.bot
            .answer_inline_query(&self.q.id, results)
            .cache_time(0)
            .await?;

        Ok(())
    }

    async fn make_sell_article(
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
        .description(self.make_sell_description(product, item).await)
        .hide_url(true)
        .thumb_url(item.image_url.clone().parse().unwrap()))
    }

    async fn make_sell_description(&mut self, product: &Product, item: &Item) -> String {
        // Assign with an inline description
        let description = item.inline_desc.clone();
        let mut info = vec![];

        // Add price
        let price = if product.negotiated_price {
            localize!(self.warehouse, &self.lang_code, "Negotiated".to_owned())
        } else {
            product.currency.format(&product.price.to_string())
        };

        match product.payment_method {
            PaymentMethod::Cash => info.push(format!("{} 💵", price)),
            PaymentMethod::Card => info.push(format!("{} 💳", price)),
            PaymentMethod::Both => info.push(format!("{}", price)),
        };

        // Add amount sold
        info.push(localize!(
            self.warehouse,
            &self.lang_code,
            "{amount} sold",
            "amount" => product.amount_sold
        ));

        // Add amount left
        info.push(localize!(
            self.warehouse,
            &self.lang_code,
            "{amount} left",
            "amount" => product.amount_left
        ));

        format!("{}\n{}", description, info.join(" • "))
    }
}
