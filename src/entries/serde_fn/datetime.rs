use chrono::{DateTime, TimeZone, Utc};
use serde::{de, Deserialize, Deserializer, Serializer};

macro_rules! de_error {
    ($($arg:tt)*) => {
        de::Error::custom(format!($($arg)*))
    };
}

pub fn serialize<S: Serializer>(
    datetime: &DateTime<Utc>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(datetime.format("%d.%m.%Y %-H:%M:%S").to_string().as_str())
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<DateTime<Utc>, D::Error> {
    let string = String::deserialize(deserializer)?;

    Utc.datetime_from_str(&string, "%d.%m.%Y %-H:%M:%S")
        .map_err(|_| de_error!("unable to parse the datetime \"{}\"", string))
}
