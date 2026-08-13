//! `Hid` + `Slug` newtypes.
//!
//! Both wrap `String`. Their purpose is to make argument-order swaps a
//! compile error: APIs that took two `&str` (one hid, one slug) used to
//! silently accept them in either order. With these distinct types, the
//! compiler refuses the swap.
//!
//! Wire compatibility: `#[serde(transparent)]` preserves JSON shape, and
//! `ToSql` / `FromSql` delegate to the inner string so SQL bindings and
//! row reads need no changes.

use std::borrow::Cow;
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

use rusqlite::types::{FromSql, FromSqlResult, ToSqlOutput, ValueRef};
use rusqlite::ToSql;
use serde::{Deserialize, Serialize};

/// Random 12-char base36 entry id. PRIMARY KEY in `entries`.
///
/// The durable identity, local and cross-device: a sync merge keys on the
/// hid and never rewrites it (da-zlug). [`Slug`] is the mutable, human-facing
/// handle.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct Hid(pub String);

/// Human-facing entry reference (`field:<ident>`). UNIQUE-indexed in `entries`.
/// Derived from field + occur_time and rewritten in place by `retime`, so it
/// is a handle, not an identity — see [`Hid`] for the stable one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct Slug(pub String);

macro_rules! impl_string_newtype {
    ($t:ident) => {
        impl $t {
            #[inline]
            pub fn new(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            #[inline]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            #[inline]
            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $t {
            #[inline]
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $t {
            #[inline]
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl Deref for $t {
            type Target = str;
            #[inline]
            fn deref(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $t {
            #[inline]
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $t {
            #[inline]
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }

        impl From<$t> for String {
            #[inline]
            fn from(v: $t) -> String {
                v.0
            }
        }

        impl<'a> From<&'a $t> for &'a str {
            #[inline]
            fn from(v: &'a $t) -> &'a str {
                &v.0
            }
        }

        impl FromStr for $t {
            type Err = std::convert::Infallible;
            #[inline]
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(s.to_string()))
            }
        }

        impl PartialEq<str> for $t {
            #[inline]
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }

        impl PartialEq<&str> for $t {
            #[inline]
            fn eq(&self, other: &&str) -> bool {
                self.0 == *other
            }
        }

        impl PartialEq<String> for $t {
            #[inline]
            fn eq(&self, other: &String) -> bool {
                &self.0 == other
            }
        }

        impl PartialEq<$t> for str {
            #[inline]
            fn eq(&self, other: &$t) -> bool {
                self == other.0.as_str()
            }
        }

        impl PartialEq<$t> for String {
            #[inline]
            fn eq(&self, other: &$t) -> bool {
                self == &other.0
            }
        }

        impl ToSql for $t {
            #[inline]
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(ToSqlOutput::Borrowed(ValueRef::Text(self.0.as_bytes())))
            }
        }

        impl FromSql for $t {
            #[inline]
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                String::column_result(value).map(Self)
            }
        }

        impl<'a> From<&'a $t> for Cow<'a, str> {
            #[inline]
            fn from(v: &'a $t) -> Cow<'a, str> {
                Cow::Borrowed(&v.0)
            }
        }
    };
}

impl_string_newtype!(Hid);
impl_string_newtype!(Slug);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hid_and_slug_are_distinct_types() {
        fn takes_slug(_: &Slug) {}
        fn takes_hid(_: &Hid) {}
        let h: Hid = "abc123".into();
        let s: Slug = "field:val".into();
        takes_hid(&h);
        takes_slug(&s);
    }

    #[test]
    fn deref_to_str_works_in_string_methods() {
        let h: Hid = Hid::from("abcdef");
        assert!(h.starts_with("abc"));
        assert_eq!(h.len(), 6);
    }

    #[test]
    fn serde_is_transparent() {
        let h = Hid::from("xyz");
        let json = serde_json::to_string(&h).unwrap();
        assert_eq!(json, "\"xyz\"");
        let back: Hid = serde_json::from_str(&json).unwrap();
        assert_eq!(back, h);
    }

    #[test]
    fn display_and_fromstr_roundtrip() {
        let s = Slug::from("field:2024");
        let rendered = format!("{s}");
        assert_eq!(rendered, "field:2024");
        let parsed: Slug = rendered.parse().unwrap();
        assert_eq!(parsed, s);
    }

    #[test]
    fn equals_str_and_string() {
        let h = Hid::from("abc");
        assert_eq!(h, "abc");
        assert_eq!(h, "abc".to_string());
    }
}
