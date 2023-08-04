use std::{fmt::Display, str::FromStr};

use lazy_static::lazy_static;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use regex::Regex;

use crate::entries::*;

lazy_static! {
    pub static ref PAYLOAD_RE: Regex = Regex::new(concat!(
        r"^(?<op>\d*)",
        r"(\sp(?<product_id>\d*))?",
        r"(\so(?<order_id>[^\s]*))?",
        r"(\sa(?<amount>\d*))?"
    ))
    .unwrap();
}

macro_rules! write_arg {
    ($args:expr, $prefix:expr, $item:expr) => {
        if let Some(some) = $item.as_ref() {
            $args.push(format!("{}{}", $prefix, some));
        };
    };
}
#[derive(Default, Debug, Clone)]
pub struct Payload {
    pub op: PayloadOp,
    pub product_id: Option<ProductId>,
    pub order_id: Option<OrderId>,
    pub amount: Option<u32>,
}

impl Payload {
    pub fn purchase(product_id: ProductId) -> Self {
        Self {
            op: PayloadOp::Purchase,
            product_id: Some(product_id),
            ..Default::default()
        }
    }

    pub fn redeem(product_id: ProductId) -> Self {
        Self {
            op: PayloadOp::Redeem,
            product_id: Some(product_id),
            ..Default::default()
        }
    }

    pub fn checkout(order_id: OrderId) -> Self {
        Self {
            op: PayloadOp::Checkout,
            order_id: Some(order_id),
            ..Default::default()
        }
    }

    pub fn cancel_order(order_id: OrderId) -> Self {
        Self {
            op: PayloadOp::CancelOrder,
            order_id: Some(order_id),
            ..Default::default()
        }
    }

    pub fn complete_order(order_id: OrderId) -> Self {
        Self {
            op: PayloadOp::CompleteOrder,
            order_id: Some(order_id),
            ..Default::default()
        }
    }

    pub fn pay_for_order(order_id: OrderId) -> Self {
        Self {
            op: PayloadOp::PayOrder,
            order_id: Some(order_id),
            ..Default::default()
        }
    }

    pub fn specify_order_price(order_id: OrderId) -> Self {
        Self {
            op: PayloadOp::SpecifyOrderPrice,
            order_id: Some(order_id),
            ..Default::default()
        }
    }
}

impl ToString for Payload {
    fn to_string(&self) -> String {
        let mut args = vec![self.op.to_string()];

        write_arg!(args, "p", self.product_id);
        write_arg!(args, "o", self.order_id);
        write_arg!(args, "a", self.amount);

        args.join(" ")
    }
}

impl FromStr for Payload {
    type Err = PayloadError;

    fn from_str(payload: &str) -> Result<Self, Self::Err> {
        let captures = PAYLOAD_RE.captures(payload);
        let payload = payload.to_owned();

        if let Some(captures) = captures {
            let op: PayloadOp = FromPrimitive::from_u8(
                captures
                    .name("op")
                    .ok_or(PayloadError::InvalidOp(payload.clone()))?
                    .as_str()
                    .parse::<u8>()
                    .map_err(|_| PayloadError::InvalidOp(payload.clone()))?,
            )
            .ok_or(PayloadError::InvalidOp(payload.clone()))?;

            let product_id = captures
                .name("product_id")
                .map(|s| s.as_str().parse::<ProductId>())
                .transpose()
                .map_err(|_| PayloadError::InvalidProductId(payload.clone()))?;

            let order_id = captures
                .name("order_id")
                .map(|s| s.as_str().parse::<OrderId>())
                .transpose()
                .map_err(|_| PayloadError::InvalidProductId(payload.clone()))?;

            let amount = captures
                .name("amount")
                .map(|s| s.as_str().parse::<u32>())
                .transpose()
                .map_err(|_| PayloadError::InvalidAmount(payload.clone()))?;

            Ok(Payload {
                op,
                product_id,
                order_id,
                amount,
            })
        } else {
            Err(PayloadError::InvalidPayload("".to_owned()))
        }
    }
}

#[derive(Default, Copy, Clone, Debug, PartialEq, PartialOrd, Eq, Ord, FromPrimitive)]
pub enum PayloadOp {
    #[default]
    None = 0,
    Purchase,
    Redeem,
    Checkout,
    CancelOrder,
    CompleteOrder,
    PayOrder,
    SpecifyOrderPrice,
}

impl PayloadOp {
    pub fn is_in_payload(&self, haystack: &str) -> bool {
        haystack.starts_with(&self.to_string())
    }
}

impl ToString for PayloadOp {
    fn to_string(&self) -> String {
        (*self as u8).to_string()
    }
}

#[derive(Debug)]
pub enum PayloadError {
    InvalidPayload(String),
    InvalidOp(String),
    InvalidProductId(String),
    InvalidAmount(String),
}

impl Display for PayloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PayloadError::InvalidPayload(payload) => {
                write!(f, "Invalid payload: {}", payload)
            }
            PayloadError::InvalidOp(payload) => {
                write!(f, "Invalid op in payload: {}", payload)
            }
            PayloadError::InvalidProductId(payload) => {
                write!(f, "Invalid product id in payload: {}", payload)
            }
            PayloadError::InvalidAmount(payload) => {
                write!(f, "Invalid amount in payload: {}", payload)
            }
        }
    }
}

impl std::error::Error for PayloadError {}
