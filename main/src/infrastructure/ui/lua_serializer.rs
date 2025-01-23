use serde::ser::Impossible;
use serde::{ser, Serialize};
use std::fmt::{Display, Formatter};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Message(String),
    Unsupported(&'static str),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use Error::*;
        match self {
            Message(m) => f.write_str(m),
            Unsupported(kind) => {
                write!(f, "serializing {kind} is currently not supported")
            }
        }
    }
}

impl ser::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Message(msg.to_string())
    }
}

impl std::error::Error for Error {}

pub struct Serializer {
    output: String,
    current_indent: usize,
    has_value: bool,
    serialize_string_as_map_key: bool,
    indent: &'static str,
}

pub fn to_string<T>(value: &T) -> Result<String>
where
    T: Serialize + ?Sized,
{
    let mut serializer = Serializer {
        output: String::new(),
        current_indent: 0,
        has_value: false,
        serialize_string_as_map_key: false,
        indent: "    ",
    };
    value.serialize(&mut serializer)?;
    Ok(serializer.output)
}

impl ser::Serializer for &mut Serializer {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Impossible<(), Error>;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Impossible<(), Error>;

    fn serialize_bool(self, v: bool) -> Result<()> {
        self.output += if v { "true" } else { "false" };
        Ok(())
    }

    fn serialize_i8(self, v: i8) -> Result<()> {
        self.serialize_i64(i64::from(v))
    }

    fn serialize_i16(self, v: i16) -> Result<()> {
        self.serialize_i64(i64::from(v))
    }

    fn serialize_i32(self, v: i32) -> Result<()> {
        self.serialize_i64(i64::from(v))
    }

    // Not particularly efficient but this is example code anyway. A more
    // performant approach would be to use the `itoa` crate.
    fn serialize_i64(self, v: i64) -> Result<()> {
        self.output += &v.to_string();
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<()> {
        self.serialize_u64(u64::from(v))
    }

    fn serialize_u16(self, v: u16) -> Result<()> {
        self.serialize_u64(u64::from(v))
    }

    fn serialize_u32(self, v: u32) -> Result<()> {
        self.serialize_u64(u64::from(v))
    }

    fn serialize_u64(self, v: u64) -> Result<()> {
        self.output += &v.to_string();
        Ok(())
    }

    fn serialize_f32(self, v: f32) -> Result<()> {
        self.serialize_f64(f64::from(v))
    }

    fn serialize_f64(self, v: f64) -> Result<()> {
        self.output += &v.to_string();
        Ok(())
    }

    fn serialize_char(self, v: char) -> Result<()> {
        self.serialize_str(&v.to_string())
    }

    fn serialize_str(self, v: &str) -> Result<()> {
        if self.serialize_string_as_map_key {
            write_map_key(&mut self.output, v);
        } else {
            let contains_newlines = v.contains(&['\r', '\n'][..]);
            if contains_newlines {
                self.output += "[[\n";
                self.output += v;
                self.output += "]]";
            } else {
                self.output += "\"";
                self.output.extend(v.escape_default());
                self.output += "\"";
            }
        }
        Ok(())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        use serde::ser::SerializeSeq;
        let mut seq = self.serialize_seq(Some(v.len()))?;
        for byte in v {
            seq.serialize_element(byte)?;
        }
        seq.end()
    }

    fn serialize_none(self) -> Result<()> {
        self.output += "nil";
        Ok(())
    }

    fn serialize_some<T>(self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<()> {
        Err(Error::Unsupported("unit"))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        Err(Error::Unsupported("unit struct"))
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<()> {
        self.serialize_str(variant)
    }

    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        Err(Error::Unsupported("newtype variant"))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        self.has_value = false;
        let len = len.ok_or_else(|| Error::Message("length of seq not given".to_string()))?;
        if len > 0 {
            self.current_indent += 1;
            self.output += "{";
        }
        Ok(self)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        self.output += "{";
        Ok(self)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        self.output += "{";
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        Err(Error::Unsupported("tuple variant"))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        self.current_indent += 1;
        self.has_value = false;
        self.output += "{";
        Ok(self)
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<Self::SerializeStruct> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        Err(Error::Unsupported("struct variant"))
    }
}

impl ser::SerializeSeq for &mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.output += "\n";
        indent(&mut self.output, self.current_indent, self.indent);
        value.serialize(&mut **self)?;
        self.output += ",";
        self.has_value = true;
        Ok(())
    }

    fn end(self) -> Result<()> {
        if self.has_value {
            self.current_indent -= 1;
            self.output += "\n";
            indent(&mut self.output, self.current_indent, self.indent);
            self.output += "}";
        } else {
            // It's important to not encode an empty sequence as "{}" because this will
            // be interpreted as map on deserialization.
            // Error message: "invalid type: map, expected a sequence".
            // Solution: Use "nil". This requires the sequence (Vec) to be optional! So each
            // Vec needs to be wrapped in an Optional.
            self.output += "nil";
        }
        Ok(())
    }
}

impl ser::SerializeTuple for &mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> std::result::Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        if !self.output.ends_with('{') {
            self.output += ", ";
        }
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        self.output += "}";
        Ok(())
    }
}

impl ser::SerializeTupleStruct for &mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        if !self.output.ends_with('{') {
            self.output += ", ";
        }
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        self.output += "}";
        Ok(())
    }
}

impl ser::SerializeMap for &mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T>(&mut self, key: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.output += "\n";
        indent(&mut self.output, self.current_indent, self.indent);
        self.serialize_string_as_map_key = true;
        key.serialize(&mut **self)?;
        self.serialize_string_as_map_key = false;
        self.has_value = true;
        Ok(())
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.output += " = ";
        value.serialize(&mut **self)?;
        self.output += ",";
        Ok(())
    }

    fn end(self) -> Result<()> {
        self.current_indent -= 1;
        if self.has_value {
            self.output += "\n";
            indent(&mut self.output, self.current_indent, self.indent);
        }
        self.output += "}";
        Ok(())
    }
}

impl ser::SerializeStruct for &mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        ensure_proper_identifier(key)?;
        self.output += "\n";
        indent(&mut self.output, self.current_indent, self.indent);
        self.output += key;
        self.output += " = ";
        value.serialize(&mut **self)?;
        self.output += ",";
        self.has_value = true;
        Ok(())
    }

    fn end(self) -> Result<()> {
        self.current_indent -= 1;
        if self.has_value {
            self.output += "\n";
            indent(&mut self.output, self.current_indent, self.indent);
        }
        self.output += "}";
        Ok(())
    }
}

fn indent(wr: &mut String, n: usize, s: &str) {
    for _ in 0..n {
        wr.push_str(s);
    }
}

fn write_map_key(wr: &mut String, v: &str) {
    if ensure_proper_identifier(v).is_ok() {
        wr.push_str(v);
    } else {
        wr.push_str("[\"");
        wr.push_str(v);
        wr.push_str("\"]");
    }
}

fn ensure_proper_identifier(v: &str) -> Result<()> {
    fn is_identifier_char(ch: char) -> bool {
        ch.is_ascii_alphanumeric() || ch == '_'
    }
    let contains_non_identifier_chars = v.contains(|ch: char| !is_identifier_char(ch));
    if contains_non_identifier_chars {
        return Err(Error::Message(format!(
            "can't serialize string {v:?} as identifier"
        )));
    }
    if LUA_KEYWORDS.contains(&v) {
        return Err(Error::Message(format!("{v:?} is a Lua identifier")));
    }
    Ok(())
}

const LUA_KEYWORDS: [&str; 21] = [
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local",
    "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];
