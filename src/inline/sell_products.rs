use tables::prelude::*;
use teloxide::prelude::*;
use teloxide::types::{
    InlineQueryResult, InlineQueryResultArticle, InputMessageContent, InputMessageContentText,
    ParseMode,
};

use crate::entries::search_group;
use crate::prelude::*;

pub async fn request(
    bot: Bot,
    q: &InlineQuery,
    warehouse: &mut Warehouse,
    user: &User,
) -> Result<()> {
    warehouse.items.refresh().await?;
    warehouse.products.refresh().await?;

    let query = q.query[5..] //skip "~sell" or "~woff"
        .trim()
        .to_lowercase()
        .split(" ")
        .map(|s| s.to_owned())
        .collect::<Vec<_>>();

    let results = warehouse
        .products
        .inner
        .read()?
        // Filter out other merchants' products
        .filter(|product| product.merchant == user.name)
        // Map item to iterator
        .filter_map(|product| {
            warehouse
                .items
                .by_id
                .get(&product.item_id)
                .map(|item| (product, item))
        })
        // Filter by query
        .filter(|(product, item)| {
            warehouse
                .items
                .search
                .get(&item.id)
                .unwrap()
                .search(&search_group::MERCHANT.to_string(), query.iter())
                || warehouse
                    .products
                    .search
                    .get(&product.id())
                    .unwrap()
                    .search(&search_group::MERCHANT.to_string(), query.iter())
        })
        // Make article
        .map(|(product, item)| Ok(InlineQueryResult::Article(make_article(&product, item)?)))
        // Maximum 50 results are allowed
        .take(50)
        .collect::<Result<Vec<_>>>()?;

    bot.answer_inline_query(&q.id, results)
        .cache_time(0)
        .await?;

    Ok(())
}

fn make_article(product: &Product, item: &Item) -> Result<InlineQueryResultArticle> {
    let content = make_product_answer(product, item);

    let content = InputMessageContent::Text(
        InputMessageContentText::new(content).parse_mode(ParseMode::Html),
    );

    Ok(
        InlineQueryResultArticle::new(format!("p?{}", product.id()), item.name.to_owned(), content)
            .description(make_description(product, item))
            .hide_url(true)
            .thumb_url(item.image_url.clone().parse().unwrap()),
    )
}

fn make_description(product: &Product, item: &Item) -> String {
    // Assign with an inline description
    let description = item.inline_desc.clone();
    let mut info = vec![];

    // Add price
    let price = if product.negotiated_price {
        "Negotiated".to_owned()
    } else {
        product.currency.format(&product.price.to_string())
    };

    match product.payment_method {
        PaymentMethod::Cash => info.push(format!("{} 💵", price)),
        PaymentMethod::Card => info.push(format!("{} 💳", price)),
        PaymentMethod::Both => info.push(format!("{}", price)),
    };

    // Add amount sold
    info.push(format!("{} sold", product.amount_sold));

    // Add amount left
    info.push(format!("{} left", product.amount_left));

    format!("{}\n{}", description, info.join(" • "))
}
