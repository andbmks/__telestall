use google_sheets4::api as sheets4;
use serde::{ser, Serializer};

use super::error::{Error, Result};

#[derive(Default)]
pub struct RowSerializer {
    pub data: Vec<sheets4::CellData>,
    seq_began: bool,
}

impl From<RowSerializer> for sheets4::RowData {
    fn from(serializer: RowSerializer) -> Self {
        sheets4::RowData {
            values: Some(serializer.data),
        }
    }
}

macro_rules! impl_ser_num {
    ($name: ident, $t0: ty) => {
        fn $name(self, v: $t0) -> Result<Self::Ok> {
            self.data.push(sheets4::CellData {
                user_entered_value: Some(sheets4::ExtendedValue {
                    number_value: Some(v as f64),
                    ..Default::default()
                }),
                ..Default::default()
            });
            Ok(())
        }
    };
}

impl Serializer for &mut RowSerializer {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    impl_ser_num!(serialize_i8, i8);
    impl_ser_num!(serialize_i16, i16);
    impl_ser_num!(serialize_i32, i32);
    impl_ser_num!(serialize_i64, i64);
    impl_ser_num!(serialize_u8, u8);
    impl_ser_num!(serialize_u16, u16);
    impl_ser_num!(serialize_u32, u32);
    impl_ser_num!(serialize_u64, u64);
    impl_ser_num!(serialize_f32, f32);
    impl_ser_num!(serialize_f64, f64);

    fn serialize_str(self, value: &str) -> Result<Self::Ok> {
        self.data.push(sheets4::CellData {
            user_entered_value: Some(sheets4::ExtendedValue {
                string_value: Some(value.to_owned()),
                ..Default::default()
            }),
            ..Default::default()
        });

        Ok(())
    }

    fn serialize_char(self, value: char) -> Result<Self::Ok> {
        self.serialize_str(&value.to_string())
    }

    fn serialize_bool(self, value: bool) -> Result<Self::Ok> {
        self.data.push(sheets4::CellData {
            user_entered_value: Some(sheets4::ExtendedValue {
                bool_value: Some(value),
                ..Default::default()
            }),
            ..Default::default()
        });
        Ok(())
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<Self::Ok> {
        Err(Error::UnexpectedBytes)
    }

    fn serialize_none(self) -> Result<Self::Ok> {
        self.data.push(sheets4::CellData {
            user_entered_value: Some(sheets4::ExtendedValue {
                string_value: Some("".to_owned()),
                ..Default::default()
            }),
            ..Default::default()
        });
        Ok(())
    }

    fn serialize_some<T: ?Sized>(self, value: &T) -> Result<Self::Ok>
    where
        T: serde::Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok> {
        self.serialize_none()
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok> {
        self.serialize_str(variant)
    }

    fn serialize_newtype_struct<T: ?Sized>(self, _name: &'static str, value: &T) -> Result<Self::Ok>
    where
        T: serde::Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok>
    where
        T: serde::Serialize,
    {
        value.serialize(self)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        if !self.seq_began {
            self.seq_began = true;
            Ok(self)
        } else {
            Err(Error::UnexpectedSequence)
        }
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        self.serialize_seq(Some(len))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        Err(Error::UnexpectedMap)
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<Self::SerializeStruct> {
        self.serialize_seq(Some(len))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        self.serialize_seq(Some(len))
    }
}

macro_rules! impl_ser_seq {
    ($ser_ty: ty, $fn_name: ident) => {
        impl $ser_ty for &mut RowSerializer {
            type Ok = ();
            type Error = Error;

            fn $fn_name<T: ?Sized>(&mut self, value: &T) -> Result<()>
            where
                T: serde::Serialize,
            {
                value.serialize(&mut **self)
            }

            fn end(self) -> Result<Self::Ok> {
                Ok(())
            }
        }
    };
}

macro_rules! impl_ser_struct {
    ($ser_ty: ty) => {
        impl $ser_ty for &mut RowSerializer {
            type Ok = ();
            type Error = Error;

            fn serialize_field<T: ?Sized>(&mut self, _key: &'static str, value: &T) -> Result<()>
            where
                T: serde::Serialize,
            {
                value.serialize(&mut **self)
            }

            fn end(self) -> Result<Self::Ok> {
                Ok(())
            }
        }
    };
}

impl_ser_seq!(ser::SerializeSeq, serialize_element);
impl_ser_seq!(ser::SerializeTuple, serialize_element);
impl_ser_seq!(ser::SerializeTupleStruct, serialize_field);
impl_ser_seq!(ser::SerializeTupleVariant, serialize_field);
impl_ser_struct!(ser::SerializeStruct);
impl_ser_struct!(ser::SerializeStructVariant);

impl ser::SerializeMap for &mut RowSerializer {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T: ?Sized>(&mut self, _key: &T) -> Result<()>
    where
        T: serde::Serialize,
    {
        Err(Error::UnexpectedMap)
    }

    fn serialize_value<T: ?Sized>(&mut self, _value: &T) -> Result<()>
    where
        T: serde::Serialize,
    {
        Err(Error::UnexpectedMap)
    }

    fn end(self) -> Result<Self::Ok> {
        Err(Error::UnexpectedMap)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde::Serialize;

    #[test]
    fn se_ok() {
        #[derive(Serialize, PartialEq, Debug)]
        enum TestEnum {
            _Variant0,
            Variant1,
        }

        #[derive(Serialize)]
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

        let test_data = TestStruct {
            text_string: "String".to_owned(),
            num_i8: 1i8,
            num_i16: 2i16,
            num_i32: 3i32,
            num_i64: 4i64,
            num_u8: 5u8,
            num_u16: 6u16,
            num_u32: 7u32,
            num_u64: 8u64,
            num_f32: 9f32,
            num_f64: 10f64,
            boolean: true,
            variants: TestEnum::Variant1,
        };

        let mut serializer = RowSerializer::default();
        test_data.serialize(&mut serializer).unwrap();
        let data = serializer.data;

        let get_val = |i: usize| data.get(i).unwrap().user_entered_value.as_ref().unwrap();
        assert_eq!(
            get_val(0).string_value.as_ref().unwrap(),
            &test_data.text_string
        );
        assert_eq!(get_val(1).number_value.unwrap(), test_data.num_i8 as f64);
        assert_eq!(get_val(2).number_value.unwrap(), test_data.num_i16 as f64);
        assert_eq!(get_val(3).number_value.unwrap(), test_data.num_i32 as f64);
        assert_eq!(get_val(4).number_value.unwrap(), test_data.num_i64 as f64);
        assert_eq!(get_val(5).number_value.unwrap(), test_data.num_u8 as f64);
        assert_eq!(get_val(6).number_value.unwrap(), test_data.num_u16 as f64);
        assert_eq!(get_val(7).number_value.unwrap(), test_data.num_u32 as f64);
        assert_eq!(get_val(8).number_value.unwrap(), test_data.num_u64 as f64);
        assert_eq!(get_val(9).number_value.unwrap(), test_data.num_f32 as f64);
        assert_eq!(get_val(10).number_value.unwrap(), test_data.num_f64 as f64);
        assert_eq!(get_val(11).bool_value.unwrap(), test_data.boolean);
        assert_eq!(
            get_val(12).string_value.as_ref().unwrap(),
            &"Variant1".to_owned()
        );
    }

    #[test]
    fn se_err() {
        #[derive(Serialize)]
        struct TestStruct {
            a: Vec<String>,
            c: String,
        }

        let test_data = TestStruct {
            a: vec!["a".into(), "b".into()],
            c: "c".into(),
        };
        let mut serializer = RowSerializer::default();

        assert!(test_data.serialize(&mut serializer).is_err());
    }

    #[test]
    fn se_custom() {
        struct TestField {
            number: u32,
            other: u32,
        }

        impl Serialize for TestField {
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer
                    .serialize_str((self.number.to_string() + &self.other.to_string()).as_str())
            }
        }

        #[derive(Serialize)]
        struct TestStruct {
            field: TestField,
        }

        let test_data = TestStruct {
            field: TestField {
                number: 123,
                other: 456,
            },
        };

        let mut serializer = RowSerializer::default();
        test_data.serialize(&mut serializer).unwrap();
        let cell = serializer.data.pop().unwrap();

        assert_eq!(
            cell.user_entered_value.unwrap().string_value.unwrap(),
            "123456".to_owned()
        );
    }
}
