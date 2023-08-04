use serde::{
    de::{self, IntoDeserializer},
    Deserializer,
};
use serde_json::Value as CellValue;

use super::error::{Error, Result};

#[derive(Clone)]
pub struct RowDeserializer<'a> {
    data: &'a [CellValue],
    seq_began: bool,
}

impl<'a> RowDeserializer<'a> {
    pub const fn new(data: &'a [CellValue]) -> Self {
        Self {
            data,
            seq_began: false,
        }
    }
}

impl<'a> RowDeserializer<'a> {
    fn next(&mut self) -> Result<&CellValue> {
        if !self.is_empty() {
            let cell = &self.data[0];
            self.data = &self.data[1..];
            Ok(cell)
        } else {
            Err(Error::OutOfBounds)
        }
    }

    fn is_empty(&self) -> bool {
        self.data.len() == 0
    }

    fn is_next_empty(&self) -> bool {
        match &self.data[0] {
            CellValue::Null => true,
            CellValue::String(s) => s.len() == 0,
            _ => false,
        }
    }

    fn parse_bool(&mut self) -> Result<bool> {
        match self.next()? {
            CellValue::Bool(v) => Ok(*v),
            CellValue::String(s) => match &s[..] {
                "TRUE" => Ok(true),
                "FALSE" => Ok(false),
                _ => Err(Error::ExpectedBoolean),
            },
            _ => Err(Error::ExpectedBoolean),
        }
    }

    fn parse_f64(&mut self) -> Result<f64> {
        match self.next()? {
            CellValue::Number(n) if n.is_f64() => Ok(n.as_f64().unwrap()),
            CellValue::String(s) => {
                let mut s = s.replace(',', ".");
                s.retain(|c| !c.is_whitespace());
                match &s {
                    s if s.ends_with('%') => s[..s.len() - 1]
                        .parse::<f64>()
                        .map(|n| n / 100.0)
                        .map_err(|_| Error::ExpectedDouble),
                    _ => s.parse::<f64>().map_err(|_| Error::ExpectedDouble),
                }
            }
            _ => Err(Error::ExpectedDouble),
        }
    }

    fn parse_u64(&mut self) -> Result<u64> {
        match self.next()? {
            CellValue::Number(n) if n.is_u64() => Ok(n.as_u64().unwrap()),
            CellValue::String(s) => s.parse::<u64>().map_err(|_| Error::ExpectedUnsigned),
            _ => Err(Error::ExpectedUnsigned),
        }
    }

    fn parse_i64(&mut self) -> Result<i64> {
        match self.next()? {
            CellValue::Number(n) if n.is_i64() => Ok(n.as_i64().unwrap()),
            CellValue::String(s) => s.parse::<i64>().map_err(|_| Error::ExpectedSigned),
            _ => Err(Error::ExpectedSigned),
        }
    }

    fn parse_str(&mut self) -> Result<&str> {
        match self.next()? {
            CellValue::String(s) => Ok(s.trim()),
            _ => Err(Error::ExpectedBoolean),
        }
    }
}

struct RowSeqAccess<'a, 'b> {
    deserializer: &'a mut RowDeserializer<'b>,
}

impl<'a, 'b> RowSeqAccess<'a, 'b> {
    fn new(deserializer: &'a mut RowDeserializer<'b>) -> Self {
        Self { deserializer }
    }
}

impl<'a, 'b, 'de> de::SeqAccess<'de> for RowSeqAccess<'a, 'b> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: de::DeserializeSeed<'de>,
    {
        if self.deserializer.is_empty() {
            return Ok(None);
        } else {
            seed.deserialize(&mut *self.deserializer)
                .map(|res| Some(res))
        }
    }
}

impl<'a, 'b, 'de> Deserializer<'de> for &'a mut RowDeserializer<'b> {
    type Error = Error;

    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_bool(self.parse_bool()?)
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_i8(self.parse_i64()? as i8)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_i16(self.parse_i64()? as i16)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_i32(self.parse_i64()? as i32)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_i64(self.parse_i64()?)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_u8(self.parse_u64()? as u8)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_u16(self.parse_u64()? as u16)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_u32(self.parse_u64()? as u32)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_u64(self.parse_u64()? as u64)
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_f32(self.parse_f64()? as f32)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_f64(self.parse_f64()?)
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_char(
            self.parse_str()?
                .chars()
                .next()
                .ok_or(Error::ExpectedChar)?,
        )
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_str(self.parse_str()?)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_string(self.parse_str()?.to_owned())
    }

    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::UnexpectedBytes)
    }

    fn deserialize_byte_buf<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        if self.is_next_empty() {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        if self.is_next_empty() {
            visitor.visit_unit()
        } else {
            Err(Error::ExpectedEmpty)
        }
    }

    fn deserialize_unit_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        if !self.seq_began {
            self.seq_began = true;
            visitor.visit_seq(RowSeqAccess::new(self))
        } else {
            Err(Error::UnexpectedSequence)
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        if !self.seq_began {
            self.seq_began = true;
            visitor.visit_seq(RowSeqAccess::new(self))
        } else {
            Err(Error::UnexpectedTuple)
        }
    }

    fn deserialize_tuple_struct<V>(
        self,
        name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        if !self.seq_began {
            self.seq_began = true;
            visitor.visit_seq(RowSeqAccess::new(self))
        } else {
            Err(Error::UnexpectedStruct(name.to_owned()))
        }
    }

    fn deserialize_map<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::UnexpectedMap)
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        if !self.seq_began {
            self.seq_began = true;
            visitor.visit_seq(RowSeqAccess::new(self))
        } else {
            Err(Error::UnexpectedStruct(name.to_owned()))
        }
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_enum(self.parse_str()?.into_deserializer())
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[test]
    fn de_ok_unformatted() {
        de_with_data(vec![
            json!("String"),
            json!(1i8),
            json!(2i16),
            json!(3i32),
            json!(4i64),
            json!(5u8),
            json!(6u16),
            json!(7u32),
            json!(8u64),
            json!(0.9f32),
            json!(250_000.2f64),
            json!(true),
            json!("Variant1"),
        ]);
    }

    #[test]
    fn de_ok_formatted() {
        de_with_data(vec![
            json!("String"),
            json!("1"),
            json!("2"),
            json!("3"),
            json!("4"),
            json!("5"),
            json!("6"),
            json!("7"),
            json!("8"),
            json!("90,00%"),
            json!("250 000,20"),
            json!("TRUE"),
            json!("Variant1"),
        ]);
    }

    fn de_with_data(data: Vec<CellValue>) {
        #[derive(Deserialize, PartialEq, Debug)]
        enum TestEnum {
            Variant0,
            Variant1,
        }

        #[derive(Deserialize)]
        struct TestStruct {
            text_string: String,
            num_i8: i8,
            num_i16: i16,
            num_i32: i32,
            num_i64: i64,
            num_u8: u8,
            num_u16: u16,
            num_u32: u32,
            num_u64: u64,
            num_f32: f32,
            num_f64: f64,
            boolean: bool,
            variants: TestEnum,
        }

        let mut deserializer = RowDeserializer::new(&data);
        let test_struct = TestStruct::deserialize(&mut deserializer).unwrap();

        assert_eq!(test_struct.text_string, "String");
        assert_eq!(test_struct.num_i8, 1i8);
        assert_eq!(test_struct.num_i16, 2i16);
        assert_eq!(test_struct.num_i32, 3i32);
        assert_eq!(test_struct.num_i64, 4i64);
        assert_eq!(test_struct.num_u8, 5u8);
        assert_eq!(test_struct.num_u16, 6u16);
        assert_eq!(test_struct.num_u32, 7u32);
        assert_eq!(test_struct.num_u64, 8u64);
        assert_eq!(test_struct.num_f32, 0.9f32);
        assert_eq!(test_struct.num_f64, 250_000.2f64);
        assert_eq!(test_struct.boolean, true);
        assert_eq!(test_struct.variants, TestEnum::Variant1);
    }

    #[test]
    fn de_err() {
        #[derive(Deserialize)]
        struct TestStruct {
            _vec: Vec<i32>,
            _map: std::collections::HashMap<String, i32>,
        }

        let data = vec![json!([1, 2, 3]), json!({"a": 1, "b": 2})];

        let mut deserializer = RowDeserializer::new(&data);
        assert!(TestStruct::deserialize(&mut deserializer).is_err());
    }

    #[test]
    fn de_custom() {
        struct TestField {
            number: u32,
        }

        impl<'de> Deserialize<'de> for TestField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                String::deserialize(deserializer).map(|s| TestField {
                    number: s.parse::<u32>().unwrap(),
                })
            }
        }

        #[derive(Deserialize)]
        struct TestStruct {
            field: TestField,
        }

        let data = vec![json!("123")];

        let mut deserializer = RowDeserializer::new(&data);
        let test_struct = TestStruct::deserialize(&mut deserializer).unwrap();

        assert_eq!(test_struct.field.number, 123);
    }
}
