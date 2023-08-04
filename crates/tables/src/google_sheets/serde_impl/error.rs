use serde;
use std::fmt::Display;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Custom(String),
    OutOfBounds,
    ExpectedBoolean,
    ExpectedString,
    ExpectedChar,
    ExpectedDouble,
    ExpectedSigned,
    ExpectedUnsigned,
    ExpectedEmpty,
    UnexpectedSequence,
    UnexpectedTuple,
    UnexpectedStruct(String),
    UnexpectedMap,
    UnexpectedBytes,
    OrphanMetadata,
}

impl serde::de::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Self::Custom(msg.to_string())
    }
}

impl serde::ser::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Self::Custom(msg.to_string())
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Custom(string) => f.write_str(string),
            Error::OutOfBounds => f.write_str("row is out of bounds"),
            Error::ExpectedBoolean => f.write_str("expected a boolean value"),
            Error::ExpectedString => f.write_str("expected a string value"),
            Error::ExpectedChar => f.write_str("expected a char value"),
            Error::ExpectedDouble => f.write_str("expected a double value"),
            Error::ExpectedSigned => f.write_str("expected a signed value"),
            Error::ExpectedUnsigned => f.write_str("expected an unsigned value"),
            Error::ExpectedEmpty => f.write_str("expected an empty cell"),
            Error::UnexpectedSequence => f.write_str("unexpected sequence"),
            Error::UnexpectedTuple => f.write_str("unexpected tuple"),
            Error::UnexpectedStruct(string) => {
                f.write_str(&format!("unexpected struct: {}", string))
            }
            Error::UnexpectedMap => f.write_str("unexpected map"),
            Error::UnexpectedBytes => f.write_str("unexpected bytes"),
            Error::OrphanMetadata => f.write_str("orphan metadata"),
        }
    }
}

impl std::error::Error for Error {}
