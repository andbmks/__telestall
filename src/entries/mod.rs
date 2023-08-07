pub mod currency;
pub mod serde_fn;

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use chrono::{DateTime, Utc};
use log::info;
use serde::{Deserialize, Serialize};

pub use currency::{Currency, CurrencyExt};
use tables::search::{Searchable, Searcher};
use teloxide::types::ChatId;

pub mod prelude {
    pub use super::{
        Currency, CurrencyExt, Item, Localization, Merchant, Order, OrderId, OrderStage,
        PaymentMethod, Product, ProductId, ProductVisibility, Replenishment, Role, Sale, SaleType,
        User, UserMeta, Writeoff,
    };
}

pub mod search_group {
    pub const USER: &'static str = "USER";
    pub const MERCHANT: &'static str = "MERCHANT";
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Product {
    pub merchant: String,
    pub item_id: String,
    pub price: f64,
    pub currency: Currency,
    pub payment_method: PaymentMethod,
    pub negotiated_price: bool,
    pub share: f32,
    pub visibility: ProductVisibility,
    pub amount_granted: u32,
    pub amount_sold: u32,
    pub amount_left: u32,
}

impl Product {
    pub fn id(&self) -> ProductId {
        Self::id_from(&self.merchant, &self.item_id)
    }

    fn id_from(merchant: &str, item_id: &str) -> ProductId {
        let mut s = DefaultHasher::new();
        merchant.hash(&mut s);
        item_id.hash(&mut s);
        s.finish()
    }

    pub fn is_visible_to(&self, user: &User) -> bool {
        match self.visibility {
            ProductVisibility::All => true,
            ProductVisibility::Personal => self.merchant == user.name,
            ProductVisibility::Merchants => user.role.is_at_least(Role::Merchant),
        }
    }

    pub fn supports_invoice(&self) -> bool {
        self.payment_method.supports_card()
    }
}

impl Searchable for Product {
    fn fill_haystack(&self, searcher: &mut Searcher) {
        match self {
            Product {
                item_id,
                merchant,
                price,
                currency,
                amount_left,
                amount_sold,
                amount_granted,
                ..
            } => {
                let fmt_price = currency.format(&price.to_string());

                searcher.write(
                    search_group::USER.to_owned(),
                    format!("by {merchant} price {fmt_price} {currency:?}").to_lowercase(),
                );
                searcher.write(
                    search_group::MERCHANT.to_owned(),
                    format!(
                        "by:{} id:{} price:{} {:?} {} left {} sold {} granted",
                        merchant,
                        item_id,
                        fmt_price,
                        currency,
                        amount_left,
                        amount_sold,
                        amount_granted
                    )
                    .to_lowercase(),
                )
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum PaymentMethod {
    Cash,
    Card,
    Both,
}

impl PaymentMethod {
    pub fn supports_card(&self) -> bool {
        match self {
            PaymentMethod::Card | PaymentMethod::Both => true,
            _ => false,
        }
    }
}

pub type ProductId = u64;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub inline_desc: String,
    pub full_desc: String,
    pub image_url: String,
}

impl Searchable for Item {
    fn fill_haystack(&self, searcher: &mut Searcher) {
        match self {
            Item {
                id,
                name,
                inline_desc,
                full_desc,
                ..
            } => {
                searcher.write(
                    search_group::USER.to_owned(),
                    format!("name:{name} desc:{inline_desc} fdesc:{full_desc}").to_lowercase(),
                );
                searcher.write(
                    search_group::MERCHANT.to_owned(),
                    format!("id:{id} name:{name} desc:{inline_desc} fdesc:{full_desc}")
                        .to_lowercase(),
                );
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ProductVisibility {
    All,
    Personal,
    Merchants,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct User {
    pub name: String,
    pub role: Role,
    pub lang_code: String,
    #[serde(with = "serde_fn::datetime")]
    pub created_date: DateTime<Utc>,
    #[serde(with = "serde_fn::datetime")]
    pub last_activity_date: DateTime<Utc>,
    pub blocked: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserMeta {
    pub name: String,
    #[serde(with = "serde_fn::chat_id")]
    pub chat_id: Option<ChatId>,
    #[serde(with = "serde_fn::list")]
    pub pending_orders: Vec<String>,
    #[serde(with = "serde_fn::list")]
    pub completed_orders: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub enum Role {
    User,
    Merchant,
    Moderator,
}

impl Role {
    pub fn is_at_least(&self, at_least: Role) -> bool {
        match at_least {
            Role::User => true,
            Role::Merchant => *self == Role::Merchant || *self == Role::Moderator,
            Role::Moderator => *self == Role::Moderator,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Merchant {
    pub name: String,
    pub location: String,
    pub address: String,
}

impl Searchable for Merchant {
    fn fill_haystack(&self, searcher: &mut Searcher) {
        match self {
            Merchant {
                location, address, ..
            } => {
                searcher.write(
                    search_group::USER.to_owned(),
                    format!("location:{location} address:{address}").to_lowercase(),
                );
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Sale {
    pub merchant: String,
    pub sale_type: SaleType,
    pub customer: String,
    pub item_id: String,
    pub comment: String,
    pub amount: u32,
    pub revenue: f64,
    pub currency: Currency,
    pub share: f32,
    #[serde(with = "serde_fn::datetime")]
    pub date: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum SaleType {
    #[serde(rename = "Hand-to-hand")]
    HandToHand,
    Order,
    Redeem,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Replenishment {
    pub supplier: String,
    pub merchant: String,
    pub item_id: String,
    pub amount: u32,
    pub cost_price: f64,
    pub currency: Currency,
    #[serde(with = "serde_fn::datetime")]
    pub date: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Writeoff {
    pub merchant: String,
    pub item_id: String,
    pub amount: u32,
    pub price: f64,
    pub currency: Currency,
    pub reason: String,
    #[serde(with = "serde_fn::datetime")]
    pub date: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Order {
    pub id: OrderId,
    pub customer: String,
    pub merchant: String,
    pub stage: OrderStage,
    pub item_id: String,
    pub amount: u32,
    pub cost: f64,
    pub currency: Currency,
    #[serde(with = "serde_fn::datetime")]
    pub date: DateTime<Utc>,
}

impl Order {
    pub fn product_id(&self) -> ProductId {
        Product::id_from(&self.merchant, &self.item_id)
    }

    pub fn into_sale(self, share: f32) -> Sale {
        Sale {
            merchant: self.merchant,
            sale_type: SaleType::Order,
            customer: self.customer,
            item_id: self.item_id,
            comment: "by order system".to_owned(),
            amount: self.amount,
            revenue: self.cost,
            currency: self.currency,
            share,
            date: Utc::now(),
        }
    }
}

impl Searchable for Order {
    fn fill_haystack(&self, searcher: &mut Searcher) {
        match self {
            Order {
                id,
                customer,
                merchant,
                stage,
                currency,
                amount,
                cost: paid,
                date,
                ..
            } => {
                let q = format!(
                    "id {id} by {customer} for {merchant} in {stage:?} x{amount} paid {paid} at {date}",
                    paid = currency.format(&paid.to_string()),
                    date = date.format("%Y-%m-%d %H:%M:%S").to_string()
                )
                .to_lowercase();

                searcher.write(search_group::USER.to_owned(), q.clone());
                searcher.write(search_group::MERCHANT.to_owned(), q);
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum OrderStage {
    Paid,
    Negotiated,
    #[serde(rename = "Wait for payment")]
    WaitForPayment,
    Completed,
    Cancelled,
}

pub type OrderId = String;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Localization {
    pub key_phrase: String,
    pub en: String,
    pub ru: String,
}

impl Localization {
    pub fn get(&self, lang: &str) -> String {
        info!("get localization for {} in {}", self.key_phrase, lang);
        match lang {
            "ru" if self.ru != "-" => self.ru.clone(),
            _ if self.en != "-" => self.en.clone(),
            _ => self.key_phrase.clone(),
        }
    }
}
