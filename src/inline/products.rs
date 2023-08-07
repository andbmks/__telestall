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
        self.warehouse.items.refresh().await?;
        self.warehouse.products.refresh().await?;
        self.warehouse.merchants.refresh().await?;

        let mut pairs: Vec<_> = self
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
            .filter(|(merchant, product, item)| {
                let mut pass = self
                    .warehouse
                    .items
                    .search
                    .get(&item.id)
                    .unwrap()
                    .search(search_group::USER, self.query.iter());

                pass |= self
                    .warehouse
                    .merchants
                    .search
                    .get(&merchant.name)
                    .unwrap()
                    .search(search_group::USER, self.query.iter());

                if self.user.role.is_at_least(Role::Merchant) {
                    pass |= self
                        .warehouse
                        .products
                        .search
                        .get(&product.id())
                        .unwrap()
                        .search(search_group::MERCHANT, self.query.iter());
                }

                pass
            })
            .skip(self.page * 50)
            .collect();

        pairs.truncate(50);
        pairs.sort_by_key(|(_, _, item)| item.name.clone());

        let mut results = vec![];

        for (merchant, product, item) in pairs {
            results.push(InlineQueryResult::Article(
                self.make_product_article(&merchant, &product, &item)
                    .await?,
            ))
        }

        if results.len() == 50 {
            if let Some(hint) = self.warehouse.items.by_id.get(&"hint_next_page".to_owned()) {
                results.pop();
                results.push(InlineQueryResult::Article(
                    InlineQueryResultArticle::new(
                        format!("p?np?{}", self.page),
                        localize!(self.warehouse, &self.lang_code, hint.name),
                        InputMessageContent::Text(InputMessageContentText::new(
                            localize!(self.warehouse, &self.lang_code, 
                                hint.full_desc, 
                                "page" => self.page + 2, 
                                "query" => self.query.join(",")))),
                    )
                    .description(
                        localize!(self.warehouse, &self.lang_code, 
                            hint.inline_desc, 
                            "page" => self.page + 2, 
                            "query" => self.query.join(",")))
                    .thumb_url(hint.image_url.clone().parse().unwrap())
                    .reply_markup(InlineKeyboardMarkup::new(vec![vec![
                        InlineKeyboardButton::switch_inline_query_current_chat(
                            localize!(self.warehouse, &self.lang_code, "Open page #{page}", "page" => self.page + 2),
                            format!("#{} {}", self.page + 2, self.query.join(","))
                        ),
                    ]])),
                ))
            }
        }

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
