use darling::FromMeta;
use std::fmt;
use std::fmt::{Display, Formatter, Write};
use syn::{
    Attribute, Field, Fields, File, GenericArgument, GenericParam, Generics, Ident, Item, ItemEnum,
    ItemStruct, PathArguments, Type, Variant,
};

pub struct LuauFile<'a> {
    rust_file: &'a File,
    include_ident: fn(&str) -> bool,
}

struct LuauItem<'a>(&'a Item);

struct LuauStruct<'a>(&'a ItemStruct);

struct LuauEnum<'a>(&'a ItemEnum);

struct RichLuauEnum<'a> {
    item_enum: &'a ItemEnum,
    /// `None` means untagged
    tag: Option<&'a str>,
}

struct PrimitiveLuauEnum<'a> {
    item_enum: &'a ItemEnum,
    ident_override: Option<String>,
}

struct LuauGenerics<'a>(&'a Generics);
struct LuauGenericArguments<'a>(&'a PathArguments);

struct LuauStructField<'a>(&'a Field);

struct LuauVariant<'a> {
    variant: &'a Variant,
    enum_ident: LuauIdent<'a>,
    /// `None` means untagged.
    tag: Option<&'a str>,
}

struct LuauVariantIdent<'a> {
    enum_ident: LuauIdent<'a>,
    variant: &'a Variant,
}

fn line_breaked<T>(value: T) -> Separated<T> {
    Separated {
        separator: "\n",
        value,
    }
}

fn separated<T>(separator: &'static str, value: T) -> Separated<T> {
    Separated { separator, value }
}

struct Separated<T> {
    separator: &'static str,
    value: T,
}

fn delimited<T>(delimiters: (&'static str, &'static str), value: T) -> Delimited<T> {
    Delimited { delimiters, value }
}

struct Delimited<T> {
    delimiters: (&'static str, &'static str),
    value: T,
}

struct LuauPrimitiveMapping<'a> {
    enum_ident: LuauIdent<'a>,
    variant: &'a Variant,
}

struct LuauType<'a>(&'a Type);

#[derive(Copy, Clone)]
struct LuauIdent<'a>(&'a Ident, Case);

#[derive(Copy, Clone)]
enum Case {
    Original,
    LowerCamelCase,
    UpperCamelCase,
    SnakeCase,
}

impl<'a> LuauFile<'a> {
    pub fn new(rust_file: &'a File, include_ident: fn(&str) -> bool) -> Self {
        Self {
            rust_file,
            include_ident,
        }
    }
}

impl<'a> Display for LuauFile<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str("local module = {}\n\n")?;
        line_breaked(|| {
            self.rust_file
                .items
                .iter()
                .filter(|item| item_is_included(item, self.include_ident))
                .map(LuauItem)
        })
        .fmt(f)?;
        f.write_str("return module\n")?;
        Ok(())
    }
}

fn item_is_included(item: &Item, include_ident: fn(&str) -> bool) -> bool {
    let ident = match item {
        Item::Enum(e) => &e.ident,
        Item::Struct(s) => &s.ident,
        Item::Type(t) => &t.ident,
        _ => return true,
    };
    include_ident(&ident.to_string())
}

impl<'a> Display for LuauIdent<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.1 {
            Case::Original => {
                self.0.fmt(f)?;
            }
            Case::LowerCamelCase => {
                let ident = heck::AsLowerCamelCase(self.0.to_string());
                ident.fmt(f)?;
            }
            Case::UpperCamelCase => {
                let ident = heck::AsUpperCamelCase(self.0.to_string());
                ident.fmt(f)?;
            }
            Case::SnakeCase => {
                let ident = heck::AsSnakeCase(self.0.to_string());
                ident.fmt(f)?;
            }
        }
        Ok(())
    }
}

impl<'a> Display for LuauItem<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match &self.0 {
            Item::Enum(e) => LuauEnum(e).fmt(f)?,
            Item::Struct(s) => LuauStruct(s).fmt(f)?,
            Item::Type(_) => {}
            _ => {}
        }
        Ok(())
    }
}

impl<'a> Display for LuauStruct<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let ident = LuauIdent(&self.0.ident, Case::Original);
        let generics = LuauGenerics(&self.0.generics);
        match &self.0.fields {
            Fields::Named(fields) => {
                let fields = line_breaked(|| fields.named.iter().map(LuauStructField));
                writeln!(
                    f,
                    r#"
                        export type {ident}{generics} = {{
                            {fields}
                        }}
                    "#
                )?;
            }
            Fields::Unnamed(fields) => {
                let field = fields
                    .unnamed
                    .first()
                    .expect("empty tuple structs not supported");
                let ty = LuauType(&field.ty);
                if fields.unnamed.len() == 1 {
                    writeln!(f, "export type {ident}{generics} = {ty};")?;
                } else {
                    let fields_equal = fields.unnamed.iter().all(|f| f == field);
                    if !fields_equal {
                        panic!("tuple structs where fields have different types are not supported: {ident}");
                    }
                    writeln!(f, "export type {ident}{generics} = {{ {ty} }};")?;
                }
            }
            Fields::Unit => {
                panic!("unit structs not supported: {}", ident);
            }
        }
        Ok(())
    }
}

#[derive(Debug, darling::FromMeta)]
pub struct SerdeTagArgs {
    tag: Option<String>,
    #[darling(default)]
    untagged: bool,
}

impl<'a> Display for LuauEnum<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let args = self.0.attrs.iter().find_map(|attr| {
            if attr.path().segments.last()?.ident != "serde" {
                return None;
            }
            Some(SerdeTagArgs::from_meta(&attr.meta).expect("unknown serde attribute"))
        });
        match args {
            None => {
                // Primitive enum
                let primitive_enum = PrimitiveLuauEnum {
                    item_enum: self.0,
                    ident_override: None,
                };
                primitive_enum.fmt(f)?;
            }
            Some(args) => {
                match (args.tag, args.untagged) {
                    (None, false) => {
                        panic!("serde attribute containing neither tag nor untagged")
                    }
                    (None, true) => {
                        // Untagged enum
                        let rich_enum = RichLuauEnum {
                            item_enum: self.0,
                            tag: None,
                        };
                        rich_enum.fmt(f)?;
                    }
                    (Some(tag), _) => {
                        // Tagged enum
                        let rich_enum = RichLuauEnum {
                            item_enum: self.0,
                            tag: Some(&tag),
                        };
                        rich_enum.fmt(f)?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl<'a> Display for RichLuauEnum<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let enum_ident = LuauIdent(&self.item_enum.ident, Case::Original);
        let generics = LuauGenerics(&self.item_enum.generics);
        let has_type_params = generics.has_type_params();
        if has_type_params {
            panic!(
                "generics not fully supported at the moment, but could be done I guess: {}",
                enum_ident
            );
        }
        let variants = line_breaked(|| {
            self.item_enum.variants.iter().map(|variant| LuauVariant {
                variant,
                enum_ident,
                tag: self.tag,
            })
        });
        let variant_disjunction = separated("|", || {
            self.item_enum
                .variants
                .iter()
                .map(|variant| LuauVariantIdent {
                    enum_ident,
                    variant,
                })
        });
        writeln!(
            f,
            r#"
                    {variants}
                    
                    export type {enum_ident} = {variant_disjunction}
                "#
        )?;
        Ok(())
    }
}

impl<'a> Display for PrimitiveLuauEnum<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let ident = if let Some(ovr) = &self.ident_override {
            ovr.clone()
        } else {
            LuauIdent(&self.item_enum.ident, Case::Original).to_string()
        };
        let variant_disjunction = separated("|", || {
            self.item_enum.variants.iter().map(|variant| {
                delimited(
                    DOUBLE_QUOTES,
                    LuauIdent(&variant.ident, Case::UpperCamelCase),
                )
            })
        });
        writeln!(
            f,
            r#"
                export type {ident} = {variant_disjunction}
            "#
        )?;
        Ok(())
    }
}

impl<'a> LuauGenerics<'a> {
    pub fn has_type_params(&self) -> bool {
        !self.0.params.is_empty()
    }
}

impl<'a> Display for LuauGenerics<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if !self.has_type_params() {
            return Ok(());
        }
        f.write_char('<')?;
        for (i, param) in self.0.params.iter().enumerate() {
            if i > 0 {
                f.write_str(" ,")?;
            }
            let GenericParam::Type(type_param) = param else {
                panic!("only type params supported: {:?}", param);
            };
            LuauIdent(&type_param.ident, Case::Original).fmt(f)?;
        }
        f.write_char('>')?;
        Ok(())
    }
}

impl<'a> Display for LuauGenericArguments<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let PathArguments::AngleBracketed(args) = self.0 else {
            return Ok(());
        };
        f.write_char('<')?;
        for (i, arg) in args.args.iter().enumerate() {
            if i > 0 {
                f.write_str(" ,")?;
            }
            let GenericArgument::Type(ty) = arg else {
                panic!("only type arguments supported: {:?}", arg);
            };
            LuauType(ty).fmt(f)?;
        }
        f.write_char('>')?;
        Ok(())
    }
}

impl<F, I, D> Display for Separated<F>
where
    F: Fn() -> I,
    I: Iterator<Item = D>,
    D: Display,
{
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let sep = self.separator;
        for (i, item) in (self.value)().enumerate() {
            if i == 0 {
                item.fmt(f)?;
            } else {
                write!(f, "{sep}{}", item)?;
            }
        }
        Ok(())
    }
}

impl<T: Display> Display for Delimited<T> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "{}{}{}",
            self.delimiters.0, self.value, self.delimiters.1
        )
    }
}

impl<'a> Display for LuauStructField<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let ty = LuauType(&self.0.ty);
        let ident = self
            .0
            .ident
            .as_ref()
            .expect("no tuple struct field expected");
        let json_ident = LuauIdent(ident, Case::SnakeCase);
        write!(f, "{json_ident}: {ty},")?;
        Ok(())
    }
}

impl<'a> Display for LuauVariantIdent<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let enum_ident = self.enum_ident;
        let variant_ident = LuauIdent(&self.variant.ident, Case::UpperCamelCase);
        write!(f, "{enum_ident}_{variant_ident}")
    }
}

impl<'a> Display for LuauVariant<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let variant_ident = LuauVariantIdent {
            enum_ident: self.enum_ident,
            variant: self.variant,
        };
        write!(f, "export type {variant_ident} = ")?;
        let upper_ident = LuauIdent(&self.variant.ident, Case::UpperCamelCase);
        if let Some(tag) = self.tag {
            // Tagged enum
            write!(f, r#"{{ {tag}: "{upper_ident}" }}"#)?;
        }
        match &self.variant.fields {
            Fields::Named(fields) => {
                // Struct-like enum variant
                if self.tag.is_some() {
                    // Tagged
                    f.write_str(" & ")?;
                }
                let fields = line_breaked(|| fields.named.iter().map(LuauStructField));
                write!(
                    f,
                    r#"
                        {{
                            {fields}
                        }}
                    "#
                )?
            }
            Fields::Unnamed(fields) => {
                assert_eq!(
                    fields.unnamed.len(),
                    1,
                    "enum tuple variants with more than one value not supported: {}",
                    self.enum_ident
                );
                // Tuple enum variant with only one field. Very common.
                let field = fields.unnamed.first().unwrap();
                if self.tag.is_some() {
                    // Tagged
                    f.write_str(" & ")?;
                }
                write!(f, "{}", LuauType(&field.ty))?;
            }
            Fields::Unit => {
                // Primitive enum variant
                assert!(
                    self.tag.is_some(),
                    "untagged enums with primitive variants invalid"
                );
            }
        };
        Ok(())
    }
}

impl<'a> Display for LuauType<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.0 {
            Type::Path(p) => {
                let el = p.path.segments.last().unwrap();
                let ident = el.ident.to_string();
                match ident.as_str() {
                    "Option" => {
                        let ty = get_type_arg(&el.arguments, 0);
                        write!(f, "{}?", LuauType(ty))?;
                    }
                    "Vec" | "EnumSet" | "HashSet" => {
                        let ty = get_type_arg(&el.arguments, 0);
                        write!(f, "{{{}}}", LuauType(ty))?;
                    }
                    "HashMap" | "Map" => {
                        let ty1 = get_type_arg(&el.arguments, 0);
                        let ty2 = get_type_arg(&el.arguments, 1);
                        write!(f, "{{[{}]: {}}}", LuauType(ty1), LuauType(ty2))?;
                    }
                    _ => {
                        let converted_type = convert_type(&ident);
                        let generic_arguments = LuauGenericArguments(&el.arguments);
                        write!(f, "{converted_type}{generic_arguments}")?;
                    }
                }
            }
            _ => panic!("unsupported type {:?}", self.0),
        }
        Ok(())
    }
}

fn convert_type(ident: &str) -> &str {
    match ident {
        "usize" | "u8" | "u16" | "u32" | "u64" | "isize" | "i8" | "i16" | "i32" | "i64"
        | "NonZeroU32" => "number",
        "f64" | "f32" => "number",
        "PathBuf" | "NaiveDateTime" | "Version" | "String" => "string",
        "bool" => "boolean",
        // serde_json::Value
        "Value" => "any",
        _ => ident,
    }
}

fn get_type_arg(args: &PathArguments, n: usize) -> &Type {
    let PathArguments::AngleBracketed(args) = args else {
        panic!("angle-bracketed type argument expected");
    };
    let arg = args
        .args
        .iter()
        .skip(n)
        .next()
        .expect("at least one argument expected");
    let GenericArgument::Type(ty) = arg else {
        panic!("type argument expected")
    };
    ty
}

const DOUBLE_QUOTES: (&str, &str) = (DOUBLE_QUOTE, DOUBLE_QUOTE);
const DOUBLE_QUOTE: &str = "\"";
