//! Not included

use super::*;
use linked_hash_map::LinkedHashMap;

pub type Specs<T> = LinkedHashMap<String, T>;

pub fn serialize<S: Serializer, V: ToString>(
    map: &Specs<V>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let output: String = map
        .into_iter()
        .map(|t| [&t.0[..], ": ", &t.1.to_string()[..], "\n"].join(""))
        .collect();

    serializer.serialize_str(&output)
}

pub fn deserialize<'de, V: FromStr, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Specs<V>, D::Error> {
    let string = String::deserialize(deserializer)?;

    let mut map = LinkedHashMap::new();

    for (i, line) in string.split('\n').enumerate() {
        let mut pair = line.split(':');
        let key = pair
            .next()
            .ok_or(de_error!(
                "a key is expected at the line {}: \"{}\"",
                i,
                line
            ))?
            .trim();

        let value = pair
            .next()
            .ok_or(de_error!(
                "a value is expected at the line {}: \"{}\"",
                i,
                line
            ))?
            .trim();

        if pair.next().is_some() {
            return Err(de_error!(
                "an invalid separator at the line {}: \"{}\"",
                i,
                line
            ));
        }

        match map.entry(key.to_owned()) {
            linked_hash_map::Entry::Occupied(_) => {
                return Err(de_error!("ambiguous entries with the key \"{}\"", key))
            }
            linked_hash_map::Entry::Vacant(entry) => {
                entry.insert(V::from_str(value).map_err(|_| {
                    de_error!(
                        "unable to decode the entry value \"{}\" for the type {}",
                        value,
                        std::any::type_name::<V>()
                    )
                })?)
            }
        };
    }

    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize)]
    struct Wrapper<T: ToString + FromStr + std::fmt::Display> {
        #[serde(with = "specs")]
        pub entries: Specs<T>,
    }

    #[test]
    fn _specs() {
        let data = r#"{"entries": "Key0: 0\nKey1: 1"}"#;

        let wrapper: Wrapper<u32> = serde_json::from_str(data).unwrap();
        let entries = wrapper.entries;

        assert_eq!(entries.len(), 2);
        assert_eq!(entries.get("Key0"), Some(&0));
        assert_eq!(entries.get("Key1"), Some(&1));

        assert!(
            serde_json::from_str::<Wrapper<u32>>(r#"{"entries": "Key0: 0\nKey1: 1 : 2"}"#).is_err()
        );
        assert!(
            serde_json::from_str::<Wrapper<u32>>(r#"{"entries": "Key0: 0\nKey1: 1\n Key2"}"#)
                .is_err()
        );
    }

    #[test]
    fn deserialize_specs() {
        let map = LinkedHashMap::from_iter([("Key0".to_owned(), 0), ("Key1".to_owned(), 1)]);
        let wrapper = Wrapper { entries: map };

        let data = serde_json::to_string(&wrapper).unwrap();
        assert_eq!(data, r#"{"entries":"Key0: 0\nKey1: 1\n"}"#);
    }
}
