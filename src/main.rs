extern crate tables;

mod callbacks;
mod commands;
#[macro_use]
mod common;
mod config;
mod dialogues;
mod entries;
mod inline;
mod utils;
mod warehouse;

use std::fmt::Display;
use std::io::Read;
use std::str::FromStr;
use std::sync::Arc;
use std::{error::Error as StdError, fs::File};

use config::Config;
use futures::future::BoxFuture;
use google_sheets4::oauth2::ServiceAccountKey;
use log::debug;
use teloxide::error_handlers::ErrorHandler;
use toml;

use teloxide::{
    dispatching::{DpHandlerDescription, UpdateHandler},
    prelude::*,
};

mod prelude {
    pub use super::{HandlerResult, Result, UnkError};
    pub use crate::common::*;
    pub use crate::dialogues::prelude::*;
    pub use crate::entries::prelude::*;
    pub use crate::warehouse::prelude::*;
}

pub type BoxedError = Box<dyn StdError + Send + Sync>;
pub type Result<T> = std::result::Result<T, BoxedError>;
pub type HandlerResult = Handler<'static, DependencyMap, Result<()>, DpHandlerDescription>;

#[derive(Debug)]
pub enum UnkError {
    Unknown(String),
    Dialogue(String),
    Tables(String),
}

impl UnkError {
    pub fn unknown(s: &str) -> Self {
        Self::Unknown(s.to_owned())
    }

    pub fn dialogue(s: &str) -> Self {
        Self::Dialogue(s.to_owned())
    }

    pub fn tables(s: &str) -> Self {
        Self::Tables(s.to_owned())
    }
}

impl Display for UnkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnkError::Unknown(s) => write!(f, "invalid request: {}", s),
            UnkError::Dialogue(s) => write!(f, "dialogue error: {}", s),
            UnkError::Tables(s) => write!(f, "unknown error: {}", s),
        }
    }
}

impl StdError for UnkError {}

struct DisplayErrorHandler;

impl<E> ErrorHandler<E> for DisplayErrorHandler
where
    E: Display,
{
    fn handle_error(self: Arc<Self>, error: E) -> BoxFuture<'static, ()> {
        log::error!("An error occurred: {}", error);
        Box::pin(async {})
    }
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let key = read_key();
    let config = read_config(key.clone());
    let creds = read_service_account_key(key);

    let warehouse = self::warehouse::build(&config, creds).await;
    let bot = Bot::new(config.telegram.bot_token);

    let mut deps = DependencyMap::default();
    deps.insert(warehouse);
    dialogues::write_deps(&mut deps);

    Dispatcher::builder(bot, schema())
        .dependencies(deps)
        .enable_ctrlc_handler()
        .default_handler(|upd| async move {
            debug!("Unhandled update: {:?}", upd);
        })
        .error_handler(Arc::new(DisplayErrorHandler))
        .build()
        .dispatch()
        .await;
}

fn schema() -> UpdateHandler<Box<dyn std::error::Error + Send + Sync + 'static>> {
    dptree::entry()
        .branch(dialogues::handler())
        .branch(inline::handler())
        .branch(commands::handler())
        .branch(callbacks::handler())
        .endpoint(common::default_handler)
}

fn read_config(key: age::x25519::Identity) -> Config {
    let encrypted_config = File::open("config.toml.enc").expect("Can't read config");
    let config = decrypt(encrypted_config, key);
    toml::from_str(&config).expect("Can't parse config")
}

fn read_service_account_key(key: age::x25519::Identity) -> ServiceAccountKey {
    let encrypted_credentials = File::open("credentials.json.enc").expect("Can't read credentials");
    let creds = decrypt(encrypted_credentials, key);

    serde_json::from_str(&creds).expect("Can't parse credentials")
}

fn decrypt(encrypted: impl Read, key: age::x25519::Identity) -> String {
    let decryptor = match age::Decryptor::new(encrypted).expect("Can't initialize decryptor") {
        age::Decryptor::Recipients(d) => d,
        age::Decryptor::Passphrase(_) => unreachable!(),
    };

    let mut reader = decryptor
        .decrypt(std::iter::once(&key as &dyn age::Identity))
        .unwrap();

    let mut data = String::new();
    reader.read_to_string(&mut data).unwrap();

    data
}

fn read_key() -> age::x25519::Identity {
    let key = std::env::var("AGE_PRIVATE_KEY").expect("Can't find AGE_PRIVATE_KEY");
    age::x25519::Identity::from_str(&key).expect("Unable to parse key.")
}
