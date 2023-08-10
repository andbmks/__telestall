mod items;
mod orders;
mod products;
mod replenish_products;
mod sell_products;

use lazy_static::lazy_static;
use regex::Regex;
use teloxide::types::{InlineQueryResultArticle, InlineQueryResult, InputMessageContentText, InputMessageContent, InlineKeyboardMarkup, InlineKeyboardButton};
use teloxide::{prelude::*, types::InlineQuery};

use crate::prelude::*;

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

    let mut request = InlineRequest::new(bot.clone(), &q, &mut warehouse, &user, lang_code)?;

    match request.cmd.as_str() {
        "" if request.query.is_empty() => request.make_items().await?,
        ".o" => request.make_orders().await?,
        "~sell" | "~woff" if user.role.is_at_least(Role::Merchant) => {
            request.make_sells().await?
        }
        "~repl" if user.role.is_at_least(Role::Moderator) => {
            request.make_replenish().await?
        }
        _ => request.make_products().await?,
    };

    Ok(())
}

lazy_static! {
    static ref QUERY_RE: Regex =
        Regex::new(r"^\s*((?<cmd>[.~]\w*))?(\s*#(?<page>[1-9][0-9]*))?(\s*(?<query>.*?))?\s*$")
            .unwrap();
}

pub struct InlineRequest<'a> {
    bot: Bot,
    q: &'a InlineQuery,
    page: usize,
    cmd: String,
    query: Vec<String>,
    warehouse: &'a mut Warehouse,
    user: &'a User,
    lang_code: String,
}

impl<'a> InlineRequest<'a> {
    pub fn new(
        bot: Bot,
        q: &'a InlineQuery,
        warehouse: &'a mut Warehouse,
        user: &'a User,
        lang_code: String,
    ) -> Result<Self> {
        let captures = QUERY_RE.captures(&q.query).ok_or("Invalid query.")?;

        let cmd = captures
            .name("cmd")
            .map(|cmd| cmd.as_str().to_owned())
            .unwrap_or("".to_owned());

        let page = captures
            .name("page")
            .map(|page| page.as_str().parse::<usize>().ok())
            .flatten()
            .unwrap_or(1)
            - 1;

        let query = captures
            .name("query")
            .map(|query| {
                query
                    .as_str()
                    .to_lowercase()
                    .replace("+", " ")
                    .split(' ')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_owned())
                    .collect()
            })
            .unwrap_or(vec![]);

        Ok(Self {
            bot,
            q,
            page,
            cmd,
            query,
            warehouse,
            user,
            lang_code,
        })
    }

    pub async fn process_results(&mut self, results: &mut Vec<InlineQueryResult>)
    {
        if results.len() == 49 {
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
                                "query" => self.query.join(" "),
                                "cmd" => self.cmd))),
                    )
                    .description(
                        localize!(self.warehouse, &self.lang_code, 
                            hint.inline_desc, 
                            "page" => self.page + 2, 
                            "query" => "",
                            "cmd" => ""))
                    .thumb_url(hint.image_url.clone().parse().unwrap())
                    .reply_markup(InlineKeyboardMarkup::new(vec![vec![
                        InlineKeyboardButton::switch_inline_query_current_chat(
                            localize!(self.warehouse, &self.lang_code, "Open page #{page}", "page" => self.page + 2),
                            format!("{} #{} {}", self.cmd, self.page + 2, self.query.join(" "))
                        ),
                    ]])),
                ))
            }
        }

    }
}
