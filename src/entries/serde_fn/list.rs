use std::str::FromStr;

use itertools::Itertools;
use serde::{de, Deserialize, Deserializer, Serializer};

pub fn serialize<S: Serializer, T: ToString>(
    list: &Vec<T>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if list.is_empty() {
        return serializer.serialize_str("-");
    }

    let text: String = list.iter().map(|item| item.to_string()).join(", ");
    serializer.serialize_str(text.as_str())
}

pub fn deserialize<'de, D: Deserializer<'de>, T: FromStr>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    let string = String::deserialize(deserializer)?;

    if string == "-" {
        return Ok(Vec::new());
    }

    string
        .split(", ")
        .map(|item| {
            item.parse()
                .map_err(|_| de::Error::custom(format!("unable to parse the item \"{}\"", item)))
        })
        .collect()
}
