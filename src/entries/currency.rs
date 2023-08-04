use std::error::Error as StdError;
use std::fmt::Display;

use serde::{
    de::{self, IntoDeserializer},
    Deserialize,
};
pub use teloxide::types::Currency;

#[derive(Debug)]
pub enum CurrencyError {
    Custom(String),
}

impl Display for CurrencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CurrencyError::Custom(msg) => f.write_str(msg),
        }
    }
}

impl de::Error for CurrencyError {
    fn custom<T: Display>(msg: T) -> Self {
        CurrencyError::Custom(msg.to_string())
    }
}

impl StdError for CurrencyError {}

pub trait CurrencyExt {
    fn format(&self, price: &str) -> String;
    fn parse(currency: &str) -> Result<Self, CurrencyError>
    where
        Self: Sized;
    fn to_string(&self) -> String;
}

impl CurrencyExt for Currency {
    fn format(&self, price: &str) -> String {
        match self {
            Currency::EUR => format!("€{}", price),
            Currency::USD => format!("${}", price),
            Currency::CZK => format!("Kč {}", price),
            Currency::UAH => format!("{}₴", price),
            Currency::KZT => format!("{}₸", price),
            Currency::RUB => format!("{}₽", price),
            _ => format!("{} {:x?}", price, self),
        }
    }

    fn parse(currency: &str) -> Result<Self, CurrencyError> {
        let de: de::value::StrDeserializer<'_, CurrencyError> = currency.into_deserializer();
        Self::deserialize(de)
    }

    fn to_string(&self) -> String {
        format!("{:x?}", self)
    }
}
