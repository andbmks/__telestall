use serde::{de, Deserialize, Deserializer, Serializer};
use teloxide::types::ChatId;

pub fn serialize<S: Serializer>(
    chat_id: &Option<ChatId>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match chat_id {
        Some(chat_id) => serializer.serialize_str(chat_id.0.to_string().as_str()),
        None => serializer.serialize_str("Unknown"),
    }
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<ChatId>, D::Error> {
    let string = String::deserialize(deserializer)?;

    match string.as_str() {
        "Unknown" => Ok(None),
        _ => Ok(Some(ChatId(string.parse().map_err(|_| {
            de::Error::custom(format!("unable to parse the chat id \"{}\"", string))
        })?))),
    }
}
