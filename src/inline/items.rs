use tables::prelude::*;
use teloxide::prelude::*;
use teloxide::types::{
    InlineKeyboardButton, InlineKeyboardMarkup, InlineQueryResult, InlineQueryResultArticle,
    InputMessageContent, InputMessageContentText, ParseMode,
};

use crate::entries::{CurrencyExt, Item, Product};
use crate::Result;

use super::InlineRequest;

impl<'a> InlineRequest<'a> {
    pub async fn make_items(&mut self) -> Result<()> {
        let mut pairs: Vec<_> = self
            .warehouse
            .products
            .by_item_id
            .all()
            // Filter out invisible & out of stock items
            .filter(|(_, products)| {
                products
                    .iter()
                    .any(|(_, p)| p.is_visible_to(self.user) && p.amount_left > 0)
            })
            // Map item to iterator
            .filter_map(|(item_id, products)| {
                self.warehouse
                    .items
                    .by_id
                    .get(item_id)
                    .map(|item| (item.clone(), products.clone()))
            })
            .skip(self.page * 49)
            .take(49)
            .collect();

        pairs.sort_by_key(|(item, _)| item.name.clone());

        let mut results = vec![];

        for (item, products) in pairs {
            results.push(InlineQueryResult::Article(
                self.make_item_article(&item, &products).await?,
            ));
        }

        self.process_results(&mut results).await;

        self.bot
            .answer_inline_query(&self.q.id, results)
            .cache_time(60)
            .await?;

        Ok(())
    }

    async fn make_item_article(
        &mut self,
        item: &Item,
        products: &Vec<(usize, Product)>,
    ) -> Result<InlineQueryResultArticle> {
        if products.len() == 1 {
            let product = &products[0].1;
            let merchant = self
                .warehouse
                .merchants
                .by_name
                .get(&product.merchant)
                .ok_or("Merchant not found")?
                .clone();

            return self.make_product_article(&merchant, product, item).await;
        }

        Ok(InlineQueryResultArticle::new(
            format!("i{}", item.id),
            localize!(self.warehouse, &self.lang_code, item.name),
            self.make_item_content(item, products).await,
        )
        .description(self.make_item_description(item, products).await)
        .reply_markup(self.make_item_markup(item).await)
        .hide_url(true)
        .thumb_url(item.image_url.clone().parse()?))
    }

    async fn make_item_content(
        &mut self,
        item: &Item,
        products: &Vec<(usize, Product)>,
    ) -> InputMessageContent {
        let price = {
            match Self::price_range_fmt(products) {
                Some((min, max)) => {
                    localize!(
                        self.warehouse,
                        &self.lang_code,
                        "ranging from <b>{min}</b> to <b>{max}</b> per item.",
                        "min" => min,
                        "max" => max
                    )
                }
                None => localize!(self.warehouse, &self.lang_code, "Negotiated."),
            }
        };

        let description = [
            format!(
                "<b>{}</b>",
                localize!(self.warehouse, &self.lang_code, item.name)
            ),
            localize!(self.warehouse, &self.lang_code, item.full_desc.clone()),
        ]
        .join("\n");

        let details = [
            localize!(self.warehouse, &self.lang_code, "<b>Details</b>"),
            localize!(self.warehouse, &self.lang_code, "• Price: {price}", "price" => price),
            localize!(self.warehouse, &self.lang_code, "• Sellers available: {cnt}", "cnt" => products.len()),
        ]
        .join("\n");

        InputMessageContent::Text(
            InputMessageContentText::new(format!("{description}\n\n{details}"))
                .parse_mode(ParseMode::Html),
        )
    }

    async fn make_item_description(
        &mut self,
        item: &Item,
        products: &Vec<(usize, Product)>,
    ) -> String {
        // Assign with an inline description
        let description = localize!(self.warehouse, &self.lang_code, item.inline_desc.clone());
        let mut info = vec![];

        // Add price
        if let Some(product) = Self::choose_best_product(products) {
            let price = product.currency.format(&product.price.to_string());
            info.push(format!("{}", price));
        }

        // Add seller count
        let product_count = products
            .iter()
            .filter(|(_, p)| p.is_visible_to(self.user) && p.amount_left > 0)
            .count();

        let ending = if product_count == 1 { "" } else { "s" };
        info.push(localize!(self.warehouse, &self.lang_code, "{product_count} seller{end}", "product_count" => product_count, "end" => ending));

        format!("{}\n{}", description, info.join(" • "))
    }

    fn price_range_fmt(products: &Vec<(usize, Product)>) -> Option<(String, String)> {
        let (mut min, mut max) = (f64::MAX, f64::MIN);
        let (mut min_idx, mut max_idx) = (usize::MAX, usize::MAX);

        products
            .iter()
            .enumerate()
            .map(|(i, (_, p))| (i, p))
            .filter(|(_, p)| !p.negotiated_price)
            .for_each(|(i, p)| {
                if p.price < min {
                    min = p.price;
                    min_idx = i;
                }
                if p.price > max {
                    max = p.price;
                    max_idx = i;
                }
            });

        if min_idx == usize::MAX || max_idx == usize::MAX {
            return None;
        }

        let min_price = products[min_idx].1.currency.format(&min.to_string());
        let max_price = products[max_idx].1.currency.format(&max.to_string());

        Some((min_price, max_price))
    }

    fn choose_best_product(products: &Vec<(usize, Product)>) -> Option<&Product> {
        products
            .iter()
            .map(|(_, p)| p)
            .filter(|p| !p.negotiated_price)
            .min_by(|a, b| a.price.partial_cmp(&b.price).unwrap())
    }

    async fn make_item_markup(&mut self, item: &Item) -> InlineKeyboardMarkup {
        let mut markup = InlineKeyboardMarkup::default();

        markup = markup.append_row(vec![
            InlineKeyboardButton::switch_inline_query_current_chat(
                localize!(self.warehouse, &self.lang_code, "Select"),
                format!("#1 name:{}", item.name.replace(" ", "+")),
            ),
        ]);

        markup
    }
}
