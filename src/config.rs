use serde::Deserialize;

use crate::tables::google_sheets::SheetArgs;

#[derive(Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
}

#[derive(Deserialize)]
pub struct SheetsConfig {
    pub spreadsheet_id: String,
    pub clock_ttl: usize,
    pub meta: SheetArgs,
    pub items: SheetArgs,
    pub products: SheetArgs,
    pub users: SheetArgs,
    pub users_meta: SheetArgs,
    pub merchants: SheetArgs,
    pub sales: SheetArgs,
    pub orders: SheetArgs,
    pub replenishments: SheetArgs,
    pub writeoffs: SheetArgs,
    pub localization: SheetArgs,
}

#[derive(Deserialize)]
pub struct Config {
    pub telegram: TelegramConfig,
    pub sheets: SheetsConfig,
}
