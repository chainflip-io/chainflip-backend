macro_rules! define_binary {
    ($module: ident, $type: ident, $size: expr, $marker: literal) => {
        mod $module {
            define_binary!(@define_struct, $type, $size);
            define_binary!(@impl_as_ref, $type, $size);
            define_binary!(@impl_from_array, $type, $size);

            #[cfg(feature = "str")]
            define_binary!(@impl_from_str, $type, $size);
            #[cfg(feature = "str")]
            define_binary!(@impl_display, $type, $size, $marker);

            #[cfg(feature = "serde")]
            define_binary!(@impl_serde_serialize, $type, $size);
            #[cfg(feature = "serde")]
            define_binary!(@impl_serde_deserialize, $type, $size);
        }
    };

    (@define_struct, $type: ident, $size: expr) => {
        #[cfg_attr(not(feature = "str"), derive(Debug))]
        #[cfg_attr(feature = "scale", derive(scale_info::TypeInfo, codec::Encode, codec::Decode, codec::MaxEncodedLen))]
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $type(pub [u8; $size]);

        impl Default for $type {
            fn default() -> Self {
                Self([0; $size])
            }
        }
    };

    (@impl_from_array, $type: ident, $size: expr) => {
        impl From<[u8; $size]> for $type {
            fn from(value: [u8; $size]) -> Self {
                Self(value)
            }
        }
        impl From<$type> for [u8; $size] {
            fn from(value: $type) -> Self {
                value.0
            }
        }
    };

    (@impl_as_ref, $type: ident, $size: expr) => {
        impl AsRef<[u8]> for $type {
            fn as_ref(&self) -> &[u8] {
                &self.0[..]
            }
        }

        impl AsRef<[u8; $size]> for $type {
            fn as_ref(&self) -> &[u8; $size] {
                &self.0
            }
        }

        impl AsMut<[u8; $size]> for $type {
            fn as_mut(&mut self) -> &mut [u8; $size] {
                &mut self.0
            }
        }
    };

    (@impl_from_str, $type: ident, $size: expr) => {
        #[cfg(feature = "str")]
        mod from_str {
            use super::*;

            impl core::str::FromStr for $type {
                type Err = ::bs58::decode::Error;

                fn from_str(s: &str) -> Result<Self, Self::Err> {
                    let mut out = Self([0u8; $size]);
                    ::bs58::decode(s).onto(&mut out.0)?;
                    Ok(out)
                }
            }
        }
    };
    (@impl_display, $type: ident, $size: expr, $marker: literal) => {
        #[cfg(feature = "str")]
        mod to_str {
            use super::*;

            impl core::fmt::Debug for $type {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    write!(f, "{}({})", $marker, self)
                }
            }

            impl core::fmt::Display for $type {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    let mut buf = [0u8; $size * 2];
                    let len = ::bs58::encode(&self.0).onto(&mut buf[..]).expect("Buffer is of sufficient size");
                    for byte in buf[..len].iter().copied() {
                        write!(f, "{}", byte as char)?;
                    }
                    Ok(())
                }
            }

        }
    };
    (@impl_serde_serialize, $type: ident, $size: expr) => {
        #[cfg(feature = "serde")]
        mod serde_ser {
            use super::*;

            use $crate::utils::WriteBuffer;

            impl serde::Serialize for $type {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: serde::Serializer,
                {
                    use core::fmt::Write;
                    use serde::ser::Error as SerError;

                    let mut buf = WriteBuffer::new([0u8; $size * 2]);
                    write!(buf, "{}", self).map_err(S::Error::custom)?;

                    let s = core::str::from_utf8(buf.as_ref()).map_err(S::Error::custom)?;

                    serializer.serialize_str(s)
                }
            }
        }
    };

    (@impl_serde_deserialize, $type: ident, $size: expr) => {
        #[cfg(feature = "serde")]
        mod serde_de {
            use super::*;

            impl<'de> serde::Deserialize<'de> for $type {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    deserializer.deserialize_str(Bs58Visitor)
                }
            }

            struct Bs58Visitor;
            impl<'de> serde::de::Visitor<'de> for Bs58Visitor {
                type Value = $type;

                fn expecting(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                    f.write_str("base58 encoded")
                }

                fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    let mut out = $type([0; $size]);
                    bs58::decode(v).onto(&mut out.0).map_err(E::custom)?;

                    Ok(out)
                }
            }
        }
    };
}
