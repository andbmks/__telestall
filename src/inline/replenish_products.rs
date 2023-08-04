use teloxide::prelude::*;
use teloxide::types::{
    InlineQueryResult, InlineQueryResultArticle, InputMessageContent, InputMessageContentText,
    ParseMode,
};

use crate::entries::search_group;
use crate::prelude::*;

pub async fn request(bot: Bot, q: &InlineQuery, warehouse: &mut Warehouse, _: &User) -> Result<()> {
    warehouse.items.refresh().await?;
    warehouse.products.refresh().await?;

    let query = q.query["~repl".len()..]
        .trim()
        .to_lowercase()
        .split(" ")
        .map(|s| s.to_owned())
        .collect::<Vec<_>>();

    let results = warehouse
        .products
        .inner
        .read()?
        // Map item to the iterator
        .filter_map(|product| {
            warehouse
                .items
                .by_id
                .get_with_row(&product.item_id)
                .map(|(_, item)| (product, item))
        })
        // Filter by query
        .filter(|(product, item)| {
            warehouse
                .items
                .search
                .get(&item.id)
                .unwrap()
                .search(search_group::MERCHANT, query.iter())
                || warehouse
                    .products
                    .search
                    .get(&product.id())
                    .unwrap()
                    .search(search_group::MERCHANT, query.iter())
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
            .description(make_description(product))
            .hide_url(true)
            .thumb_url(item.image_url.clone().parse().unwrap()),
    )
}

fn make_description(product: &Product) -> String {
    let mut line0 = vec![];
    let mut line1 = vec![];

    // Add merchant
    line0.push(format!("by @{}", product.merchant));

    // Add price
    let price = if product.negotiated_price {
        "Negotiated".to_owned()
    } else {
        product.currency.format(&product.price.to_string())
    };

    match product.payment_method {
        PaymentMethod::Cash => line0.push(format!("{} 💵", price)),
        PaymentMethod::Card => line0.push(format!("{} 💳", price)),
        PaymentMethod::Both => line0.push(format!("{}", price)),
    };

    // Add amount granted
    line1.push(format!("{} granted", product.amount_granted));

    // Add amount sold
    line1.push(format!("{} sold", product.amount_sold));

    // Add amount left
    line1.push(format!("{} left", product.amount_left));

    format!("{}\n{}", line0.join(" • "), line1.join(" • "))
}
