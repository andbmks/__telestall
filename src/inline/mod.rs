mod items;
mod orders;
mod products;
mod replenish_products;
mod sell_products;

use teloxide::{prelude::*, types::InlineQuery};

use super::{HandlerResult, Result};
use crate::common::handle_user_from_inline;
use crate::entries::Role;
use crate::prelude::User;
use crate::warehouse::{SharedWarehouse, Warehouse};

pub fn handler() -> HandlerResult {
    Update::filter_inline_query().endpoint(handle_inline_query)
}

pub async fn handle_inline_query(
    bot: Bot,
    q: InlineQuery,
    warehouse: SharedWarehouse,
) -> Result<()> {
    let mut warehouse = warehouse.write().await;

    let (user, _) = handle_user_from_inline(&mut warehouse, &q).await?;

    if user.blocked {
        return Ok(());
    }

    let lang_code = q.from.language_code.clone().unwrap_or("en".to_string());

    let collect_query = |query: &mut Vec<_>, prefix: usize| {
        query.extend(
            q.query[prefix..]
                .trim()
                .to_lowercase()
                .split(" ")
                .map(|s| s.to_owned()),
        )
    };

    let mut request = InlineRequest {
        bot: bot.clone(),
        q: &q,
        warehouse: &mut warehouse,
        user: &user,
        lang_code,
        query: Vec::new(),
    };

    match &q.query.trim()[..] {
        s if s.is_empty() => request.make_items().await?,

        s if (s.starts_with("~sell") || s.starts_with("~woff"))
            && user.role.is_at_least(Role::Merchant) =>
        {
            sell_products::request(bot, &q, &mut warehouse, &user).await?
        }
        s if s.starts_with("~repl") && user.role.is_at_least(Role::Moderator) => {
            replenish_products::request(bot, &q, &mut warehouse, &user).await?
        }
        s if s.starts_with(".o") && user.role.is_at_least(Role::User) => {
            collect_query(&mut request.query, ".o".len());
            request.make_orders().await?
        }
        _ => {
            collect_query(&mut request.query, "".len());
            request.make_products().await?
        }
    };

    Ok(())
}

pub struct InlineRequest<'a> {
    bot: Bot,
    q: &'a InlineQuery,
    warehouse: &'a mut Warehouse,
    user: &'a User,
    lang_code: String,
    query: Vec<String>,
}
