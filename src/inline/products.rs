use itertools::Itertools;
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
    pub async fn make_products(&mut self) -> Result<()> {
        let pairs: Vec<_> = self
            .warehouse
            .products
            .inner
            .read()?
            .filter(|p| p.is_visible_to(self.user) && p.amount_left > 0)
            // Map item to the iterator
            .filter_map(|product| {
                self.warehouse
                    .items
                    .by_id
                    .get(&product.item_id)
                    .map(|item| (product, item))
            })
            // Map merchants to the iterator
            .filter_map(|(product, item)| {
                self.warehouse
                    .merchants
                    .by_name
                    .group(&product.merchant)
                    .map(|merchant| (merchant.clone(), product.clone(), item.clone()))
            })
            .flat_map(|(merchants, product, item)| {
                let mut vec = vec![];

                for merchant in merchants {
                    vec.push((merchant.1, product.clone(), item.clone()));
                }
                vec
            })
            // Filter by query
            .filter_map(|(merchant, product, item)| {
                let item_searcher = self.warehouse.items.search.get(&item.id).unwrap();
                let merchant_searcher =
                    self.warehouse.merchants.search.get(&merchant.name).unwrap();

                let item_all_passsed =
                    item_searcher.search_all(search_group::USER, self.query.iter());

                let merchant_all_passed =
                    merchant_searcher.search_all(search_group::USER, self.query.iter());

                let item_any_passsed =
                    item_searcher.search_any(search_group::USER, self.query.iter());

                let merchant_any_passed =
                    merchant_searcher.search_any(search_group::USER, self.query.iter());

                let priority = match (
                    item_all_passsed,
                    item_any_passsed,
                    merchant_all_passed,
                    merchant_any_passed,
                ) {
                    (true, _, true, _) => 0,
                    (true, _, _, true) => 1,
                    (true, _, _, _) => 2,
                    (_, _, true, _) => 3,
                    (_, true, _, _) => 4,
                    (_, _, _, true) => 4,
                    (false, false, false, false) => return None,
                };

                Some((priority, merchant, product, item))
            })
            .sorted_by(|(prior_a, _, _, item_a), (prior_b, _, _, item_b)| {
                prior_a.cmp(prior_b).then(item_a.name.cmp(&item_b.name))
            })
            .skip(self.page * 49)
            .take(49)
            .collect();

        let mut results = vec![];

        for (_, merchant, product, item) in pairs {
            results.push(InlineQueryResult::Article(
                self.make_product_article(&merchant, &product, &item)
                    .await?,
            ))
        }

        self.process_results(&mut results).await;

        self.bot
            .answer_inline_query(&self.q.id, results)
            .cache_time(60)
            .await?;

        Ok(())
    }

    pub async fn make_product_article(
        &mut self,
        merchant: &Merchant,
        product: &Product,
        item: &Item,
    ) -> Result<InlineQueryResultArticle> {
        Ok(InlineQueryResultArticle::new(
            format!("p?{}{}", product.id(), merchant.location),
            localize!(self.warehouse, &self.lang_code, item.name.to_owned()),
            self.make_product_content(merchant, item, product).await,
        )
        .description(self.make_product_description(merchant, item, product).await)
        .reply_markup(self.make_product_markup(product).await)
        .hide_url(true)
        .thumb_url(item.image_url.clone().parse().unwrap()))
    }

    async fn make_product_content(
        &mut self,
        merchant: &Merchant,
        item: &Item,
        product: &Product,
    ) -> InputMessageContent {
        let price = if product.negotiated_price {
            localize!(self.warehouse, &self.lang_code, "Negotiated").to_string()
        } else {
            product.currency.format(&product.price.to_string())
        };

        let payment_method = match product.payment_method {
            PaymentMethod::Cash => localize!(self.warehouse, &self.lang_code, "Cash"),
            PaymentMethod::Card => localize!(self.warehouse, &self.lang_code, "Card"),
            PaymentMethod::Both => localize!(self.warehouse, &self.lang_code, "Cash or Card"),
        };

        const GOOGLE_MAPS_URL: &str = "https://www.google.com/maps/search/?api=1&query=";
        let address_encoded = urlencoding::encode(&merchant.address).to_string();
        let location = format!(
            "<a href=\"{GOOGLE_MAPS_URL}{}\">{}</a>.",
            address_encoded, merchant.location
        );

        let description = [format!("<b>{}</b>", item.name), item.full_desc.clone()].join("\n");

        let details = [
            "<b>Details</b>".to_string(),
            localize!(self.warehouse, &self.lang_code, "• Price: <b>{price}</b>", "price" => price),
            localize!(self.warehouse, &self.lang_code, "• Payment method: {payment_method}", "payment_method" => payment_method),
            localize!(self.warehouse, &self.lang_code, "• Location: {location}", "location" => location),
        ].join("\n");

        InputMessageContent::Text(
            InputMessageContentText::new(format!("{description}\n\n{details}"))
                .disable_web_page_preview(true)
                .parse_mode(ParseMode::Html),
        )
    }

    async fn make_product_description(
        &mut self,
        merchant: &Merchant,
        item: &Item,
        product: &Product,
    ) -> String {
        // Assign with an inline description
        let description = item.inline_desc.clone();
        let mut info = vec![];

        // Add merchant's 🛒
        if product.merchant == self.user.name {
            info.push(localize!(self.warehouse, &self.lang_code, "🛒 Your"));
        }

        // Add price
        let price = if product.negotiated_price {
            localize!(self.warehouse, &self.lang_code, "Negotiated")
        } else {
            product.currency.format(&product.price.to_string())
        };

        match product.payment_method {
            PaymentMethod::Cash => info.push(format!("{} 💵", price)),
            PaymentMethod::Card => info.push(format!("{} 💳", price)),
            PaymentMethod::Both => info.push(format!("{}", price)),
        };

        if product.merchant != self.user.name {
            info.push(format!("{} 📌", merchant.location));
        }

        format!("{}\n{}", description, info.join(" • "))
    }

    async fn make_product_markup(&mut self, product: &Product) -> InlineKeyboardMarkup {
        let mut markup = InlineKeyboardMarkup::default();

        if self.user.name == product.merchant && !product.negotiated_price {
            markup = markup.append_row(vec![InlineKeyboardButton::callback(
                localize!(self.warehouse, &self.lang_code, "Redeem"),
                Payload::redeem(product.id()).to_string(),
            )]);
        }

        if self.user.name != product.merchant {
            markup = markup.append_row(vec![InlineKeyboardButton::callback(
                localize!(self.warehouse, &self.lang_code, "Purchase"),
                Payload::purchase(product.id()).to_string(),
            )]);
        }

        markup
    }
}
