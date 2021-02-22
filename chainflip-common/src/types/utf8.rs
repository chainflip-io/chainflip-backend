use super::Bytes;
use crate::string::{String, ToString};
use codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::vec::Vec;
use std::{
    fmt,
    str::{self, FromStr},
};

/// A representation of a utf8 string
#[derive(Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Encode, Decode)]
pub struct ByteString(Bytes);

impl fmt::Debug for ByteString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let string = self.string().unwrap();
        write!(f, "{}", string)
    }
}

impl ByteString {
    fn string(&self) -> Option<String> {
        String::from_utf8(self.0.clone()).ok()
    }
}
impl FromStr for ByteString {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.as_bytes().to_vec()))
    }
}

impl From<String> for ByteString {
    fn from(s: String) -> Self {
        Self::from_str(s.as_ref()).unwrap()
    }
}

impl From<&String> for ByteString {
    fn from(s: &String) -> Self {
        Self::from_str(s.as_ref()).unwrap()
    }
}

impl From<&str> for ByteString {
    fn from(s: &str) -> Self {
        Self::from_str(s.as_ref()).unwrap()
    }
}

impl From<Vec<u8>> for ByteString {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

impl fmt::Display for ByteString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            self.string().unwrap_or("Invalid utf8 bytes".to_string())
        )
    }
}

impl Serialize for ByteString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.string() {
            Some(s) => serializer.serialize_str(&s),
            None => Err(serde::ser::Error::custom(
                "String contains invalid UTF-8 characters",
            )),
        }
    }
}

impl<'de> Deserialize<'de> for ByteString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Unexpected, Visitor};

        struct ByteStringVisitor;

        impl<'de> Visitor<'de> for ByteStringVisitor {
            type Value = ByteString;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "a valid utf-8 string")
            }

            fn visit_str<E>(self, s: &str) -> Result<ByteString, E>
            where
                E: de::Error,
            {
                ByteString::from_str(s)
                    .map_err(|_| de::Error::invalid_value(Unexpected::Str(s), &self))
            }
        }

        deserializer.deserialize_str(ByteStringVisitor)
    }
}
