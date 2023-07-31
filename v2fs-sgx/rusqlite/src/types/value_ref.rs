use super::{Type, Value};
use crate::types::{FromSqlError, FromSqlResult};
use std::boxed::Box;
use std::string::ToString;

/// A non-owning [dynamic type value](http://sqlite.org/datatype3.html). Typically the
/// memory backing this value is owned by SQLite.
///
/// See [`Value`](Value) for an owning dynamic type value.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ValueRef<'a> {
    /// The value is a `NULL` value.
    Null,
    /// The value is a signed integer.
    Integer(i64),
    /// The value is a floating point number.
    Real(f64),
    /// The value is a text string.
    Text(&'a [u8]),
    /// The value is a blob of data
    Blob(&'a [u8]),
}

impl ValueRef<'_> {
    /// Returns SQLite fundamental datatype.
    #[inline]
    #[must_use]
    pub fn data_type(&self) -> Type {
        match *self {
            ValueRef::Null => Type::Null,
            ValueRef::Integer(_) => Type::Integer,
            ValueRef::Real(_) => Type::Real,
            ValueRef::Text(_) => Type::Text,
            ValueRef::Blob(_) => Type::Blob,
        }
    }
}

impl<'a> ValueRef<'a> {
    /// If `self` is case `Integer`, returns the integral value. Otherwise,
    /// returns [`Err(Error::InvalidColumnType)`](crate::Error::
    /// InvalidColumnType).
    #[inline]
    pub fn as_i64(&self) -> FromSqlResult<i64> {
        match *self {
            ValueRef::Integer(i) => Ok(i),
            _ => Err(FromSqlError::InvalidType),
        }
    }

    /// If `self` is case `Null` returns None.
    /// If `self` is case `Integer`, returns the integral value.
    /// Otherwise returns [`Err(Error::InvalidColumnType)`](crate::Error::
    /// InvalidColumnType).
    #[inline]
    pub fn as_i64_or_null(&self) -> FromSqlResult<Option<i64>> {
        match *self {
            ValueRef::Null => Ok(None),
            ValueRef::Integer(i) => Ok(Some(i)),
            _ => Err(FromSqlError::InvalidType),
        }
    }

    /// If `self` is case `Real`, returns the floating point value. Otherwise,
    /// returns [`Err(Error::InvalidColumnType)`](crate::Error::
    /// InvalidColumnType).
    #[inline]
    pub fn as_f64(&self) -> FromSqlResult<f64> {
        match *self {
            ValueRef::Real(f) => Ok(f),
            _ => Err(FromSqlError::InvalidType),
        }
    }

    /// If `self` is case `Null` returns None.
    /// If `self` is case `Real`, returns the floating point value.
    /// Otherwise returns [`Err(Error::InvalidColumnType)`](crate::Error::
    /// InvalidColumnType).
    #[inline]
    pub fn as_f64_or_null(&self) -> FromSqlResult<Option<f64>> {
        match *self {
            ValueRef::Null => Ok(None),
            ValueRef::Real(f) => Ok(Some(f)),
            _ => Err(FromSqlError::InvalidType),
        }
    }

    /// If `self` is case `Text`, returns the string value. Otherwise, returns
    /// [`Err(Error::InvalidColumnType)`](crate::Error::InvalidColumnType).
    #[inline]
    pub fn as_str(&self) -> FromSqlResult<&'a str> {
        match *self {
            ValueRef::Text(t) => {
                std::str::from_utf8(t).map_err(|e| FromSqlError::Other(Box::new(e)))
            }
            _ => Err(FromSqlError::InvalidType),
        }
    }

    /// If `self` is case `Null` returns None.
    /// If `self` is case `Text`, returns the string value.
    /// Otherwise returns [`Err(Error::InvalidColumnType)`](crate::Error::
    /// InvalidColumnType).
    #[inline]
    pub fn as_str_or_null(&self) -> FromSqlResult<Option<&'a str>> {
        match *self {
            ValueRef::Null => Ok(None),
            ValueRef::Text(t) => std::str::from_utf8(t)
                .map_err(|e| FromSqlError::Other(Box::new(e)))
                .map(Some),
            _ => Err(FromSqlError::InvalidType),
        }
    }

    /// If `self` is case `Blob`, returns the byte slice. Otherwise, returns
    /// [`Err(Error::InvalidColumnType)`](crate::Error::InvalidColumnType).
    #[inline]
    pub fn as_blob(&self) -> FromSqlResult<&'a [u8]> {
        match *self {
            ValueRef::Blob(b) => Ok(b),
            _ => Err(FromSqlError::InvalidType),
        }
    }

    /// If `self` is case `Null` returns None.
    /// If `self` is case `Blob`, returns the byte slice.
    /// Otherwise returns [`Err(Error::InvalidColumnType)`](crate::Error::
    /// InvalidColumnType).
    #[inline]
    pub fn as_blob_or_null(&self) -> FromSqlResult<Option<&'a [u8]>> {
        match *self {
            ValueRef::Null => Ok(None),
            ValueRef::Blob(b) => Ok(Some(b)),
            _ => Err(FromSqlError::InvalidType),
        }
    }

    /// Returns the byte slice that makes up this ValueRef if it's either
    /// [`ValueRef::Blob`] or [`ValueRef::Text`].
    #[inline]
    pub fn as_bytes(&self) -> FromSqlResult<&'a [u8]> {
        match self {
            ValueRef::Text(s) | ValueRef::Blob(s) => Ok(s),
            _ => Err(FromSqlError::InvalidType),
        }
    }

    /// If `self` is case `Null` returns None.
    /// If `self` is [`ValueRef::Blob`] or [`ValueRef::Text`] returns the byte
    /// slice that makes up this value
    #[inline]
    pub fn as_bytes_or_null(&self) -> FromSqlResult<Option<&'a [u8]>> {
        match *self {
            ValueRef::Null => Ok(None),
            ValueRef::Text(s) | ValueRef::Blob(s) => Ok(Some(s)),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

impl From<ValueRef<'_>> for Value {
    #[inline]
    fn from(borrowed: ValueRef<'_>) -> Value {
        match borrowed {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(i) => Value::Integer(i),
            ValueRef::Real(r) => Value::Real(r),
            ValueRef::Text(s) => {
                let s = std::str::from_utf8(s).expect("invalid UTF-8");
                Value::Text(s.to_string())
            }
            ValueRef::Blob(b) => Value::Blob(b.to_vec()),
        }
    }
}

impl<'a> From<&'a str> for ValueRef<'a> {
    #[inline]
    fn from(s: &str) -> ValueRef<'_> {
        ValueRef::Text(s.as_bytes())
    }
}

impl<'a> From<&'a [u8]> for ValueRef<'a> {
    #[inline]
    fn from(s: &[u8]) -> ValueRef<'_> {
        ValueRef::Blob(s)
    }
}

impl<'a> From<&'a Value> for ValueRef<'a> {
    #[inline]
    fn from(value: &'a Value) -> ValueRef<'a> {
        match *value {
            Value::Null => ValueRef::Null,
            Value::Integer(i) => ValueRef::Integer(i),
            Value::Real(r) => ValueRef::Real(r),
            Value::Text(ref s) => ValueRef::Text(s.as_bytes()),
            Value::Blob(ref b) => ValueRef::Blob(b),
        }
    }
}

impl<'a, T> From<Option<T>> for ValueRef<'a>
where
    T: Into<ValueRef<'a>>,
{
    #[inline]
    fn from(s: Option<T>) -> ValueRef<'a> {
        match s {
            Some(x) => x.into(),
            None => ValueRef::Null,
        }
    }
}
