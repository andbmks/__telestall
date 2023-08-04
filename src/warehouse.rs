use chrono::Duration;
use google_sheets4::{
    hyper, hyper_rustls,
    oauth2::{ServiceAccountAuthenticator, ServiceAccountKey},
    Sheets,
};
use std::sync::Arc;
use tables::{
    cache::Cache, clock::Clock, fork, google_sheets::Sheet, in_mem::InMemTable, index::Index,
    search::Searcher,
};
use tokio::sync::RwLock;

use crate::{config::Config, entries::*};

pub mod prelude {
    pub use super::{SharedWarehouse, Warehouse};
    pub use tables::prelude::*;
}

pub type Table<E> = Cache<Clock<Sheet<E>>, InMemTable<E>>;
pub type SharedWarehouse = Arc<RwLock<Warehouse>>;

fork!(items_table: ItemTable[Item], 
      inner: Table<Item>,
      by_id: Index<String, Item>, 
      search: Index<String, Item, Searcher>);

fork!(products_table: ProductTable[Product], 
      inner: Table<Product>,
      by_item_id: Index<String, Product>,
      by_id: Index<u64, Product>,
      search: Index<u64, Product, Searcher>);

fork!(users_table: UsersTable[User], 
      inner: Table<User>,
      by_name: Index<String, User>);

fork!(users_meta_table: UsersMetaTable[UserMeta], 
      inner: Table<UserMeta>,
      by_name: Index<String, UserMeta>);

fork!(mechants_talbe: MerchantsTable[Merchant], 
      inner: Table<Merchant>,
      by_name: Index<String, Merchant>,
      search: Index<String, Merchant, Searcher>);

fork!(orders_table: OrdersTable[Order], 
      inner: Table<Order>,
      by_id: Index<String, Order>,
      search: Index<String, Order, Searcher>);

fork!(loc_table: LocalizationTable[Localization], 
      inner: Table<Localization>,
      by_key_phrase: Index<String, Localization>);

pub struct Warehouse {
    pub items: ItemTable,
    pub products: ProductTable,
    pub users: UsersTable,
    pub users_meta: UsersMetaTable,
    pub merchants: MerchantsTable,
    pub sales: Table<Sale>,
    pub orders: OrdersTable,
    pub replenishments: Table<Replenishment>,
    pub writeoffs: Table<Writeoff>,
    pub localization: LocalizationTable,
}

pub async fn build(config: &Config, creds: ServiceAccountKey) -> SharedWarehouse {
    let auth = ServiceAccountAuthenticator::builder(creds)
        .build()
        .await
        .expect("There was an error, trying to build connection with authenticator");

    let connector = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build();

    let hub = Arc::new(Sheets::new(hyper::Client::builder().build(connector), auth));

    let clock_ttl = Duration::weeks(config.sheets.clock_ttl as i64);

    Arc::new(RwLock::new(Warehouse {
        items: ItemTable::new(
            Table::new(
                Clock::new(
                    Sheet::new(
                        hub.clone(),
                        config.sheets.spreadsheet_id.clone(),
                        config.sheets.items.clone(),
                    ),
                    clock_ttl,
                ),
                [].into(),
            ),
            Index::new(|_, p: &Item| p.id.clone()),
            Index::new(|_, p: &Item| p.id.clone()),
        ),
        products: ProductTable::new(
            Table::new(
                Clock::new(
                    Sheet::new(
                        hub.clone(),
                        config.sheets.spreadsheet_id.clone(),
                        config.sheets.products.clone(),
                    ),
                    clock_ttl,
                ),
                [].into(),
            ),
            Index::new(|_, p: &Product| p.item_id.clone()),
            Index::new(|_, p: &Product| p.id()),
            Index::new(|_, p: &Product| p.id()),
        ),
        users: UsersTable::new(
            Table::new(
                Clock::new(
                    Sheet::new(
                        hub.clone(),
                        config.sheets.spreadsheet_id.clone(),
                        config.sheets.users.clone(),
                    ),
                    clock_ttl,
                ),
                [].into(),
            ),
            Index::new(|_, user| user.name.clone()),
        ),
        users_meta: UsersMetaTable::new(
            Table::new(
                Clock::new(
                    Sheet::new(
                        hub.clone(),
                        config.sheets.spreadsheet_id.clone(),
                        config.sheets.users_meta.clone(),
                    ),
                    clock_ttl,
                ),
                [].into(),
            ),
            Index::new(|_, meta| meta.name.clone()),
        ),
        merchants: MerchantsTable::new(
            Table::new(
                Clock::new(
                    Sheet::new(
                        hub.clone(),
                        config.sheets.spreadsheet_id.clone(),
                        config.sheets.merchants.clone(),
                    ),
                    clock_ttl,
                ),
                [].into(),
            ),
            Index::new(|_, merchant| merchant.name.clone()),
            Index::new(|_, merchant| merchant.name.clone()),
        ),
        sales: Table::new(
            Clock::new(
                Sheet::new(
                    hub.clone(),
                    config.sheets.spreadsheet_id.clone(),
                    config.sheets.sales.clone(),
                ),
                clock_ttl,
            ),
            [].into(),
        ),
        orders: OrdersTable::new(
            Table::new(
                Clock::new(
                    Sheet::new(
                        hub.clone(),
                        config.sheets.spreadsheet_id.clone(),
                        config.sheets.orders.clone(),
                    ),
                    clock_ttl,
                ),
                [].into(),
            ),
            Index::new(|_, order| order.id.clone()),
            Index::new(|_, order: &Order| order.id.clone()),
        ),
        replenishments: Table::new(
            Clock::new(
                Sheet::new(
                    hub.clone(),
                    config.sheets.spreadsheet_id.clone(),
                    config.sheets.replenishments.clone(),
                ),
                clock_ttl,
            ),
            [].into(),
        ),
        writeoffs: Table::new(
            Clock::new(
                Sheet::new(
                    hub.clone(),
                    config.sheets.spreadsheet_id.clone(),
                    config.sheets.writeoffs.clone(),
                ),
                clock_ttl,
            ),
            [].into(),
        ),
        localization: LocalizationTable {
            inner: Table::new(
                Clock::new(
                    Sheet::new(
                        hub.clone(),
                        config.sheets.spreadsheet_id.clone(),
                        config.sheets.localization.clone(),
                    ),
                    clock_ttl,
                ),
                [].into(),
            ),
            by_key_phrase: Index::new(|_, loc| loc.key_phrase.clone()),
        },
    }))
}
