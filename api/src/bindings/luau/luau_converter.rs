use darling::FromMeta;
use std::fmt::{Display, Formatter, Write};
use std::{fmt, iter};
use syn::{
    Field, Fields, File, GenericArgument, GenericParam, Generics, Ident, Item, ItemEnum,
    ItemStruct, PathArguments, Type, Variant,
};

pub trait Hook {
    /// Return `true` if you want to include this type in the language binding.
    ///
    /// Called for each Rust type with its simple name (not including the path).
    fn include_type(&self, simple_ident: &str) -> bool {
        let _ = simple_ident;
        true
    }

    /// Return the Luau module name that corresponds to the given Rust crate name. Return `None`
    /// if you want to use the crate's name as Luau module name.
    ///
    /// Called for each referenced Rust type that's addressed with an explicit module path. It will pass the first
    /// segment of that past. Ideally, our logic would resolve the crate name by looking at the use statements,
    /// but it's okay, we just need to be a bit disciplined when writing the Rust API code.
    fn translate_crate_name(&self, rust_crate_ident: &str) -> Option<&'static str> {
        let _ = rust_crate_ident;
        None
    }
}

pub struct LuauFile<'a, H> {
    context: Context<'a, H>,
}

struct Context<'a, H> {
    file: &'a File,
    hook: &'a H,
    foreign_files: &'a [File],
}

impl<'a, H> Context<'a, H> {
    fn find_item_by_ident(&self, needle: &Ident) -> Option<&Item> {
        iter::once(self.file)
            .chain(self.foreign_files.iter())
            .flat_map(|file| file.items.iter())
            .find(|it| match it {
                Item::Struct(ItemStruct { ident, .. }) | Item::Enum(ItemEnum { ident, .. }) => {
                    ident == needle
                }
                _ => false,
            })
    }
}

impl<'a, H> Clone for Context<'a, H> {
    fn clone(&self) -> Self {
        Self {
            file: self.file,
            hook: self.hook,
            foreign_files: self.foreign_files,
        }
    }
}

impl<'a, H> Copy for Context<'a, H> {}

struct LuauItem<'a, H> {
    value: &'a Item,
    context: Context<'a, H>,
}

impl<'a, H: Hook> LuauItem<'a, H> {
    pub fn new(value: &'a Item, context: Context<'a, H>) -> Self {
        Self { value, context }
    }
}

struct LuauStruct<'a, H> {
    value: &'a ItemStruct,
    context: Context<'a, H>,
}

impl<'a, H> LuauStruct<'a, H> {
    pub fn new(value: &'a ItemStruct, context: Context<'a, H>) -> Self {
        Self { value, context }
    }
}

struct LuauEnum<'a, H> {
    value: &'a ItemEnum,
    context: Context<'a, H>,
}

impl<'a, H> LuauEnum<'a, H> {
    pub fn new(value: &'a ItemEnum, context: Context<'a, H>) -> Self {
        Self { value, context }
    }
}

struct RichLuauEnum<'a, H> {
    item_enum: &'a ItemEnum,
    /// `None` means untagged
    tag: Option<&'a str>,
    context: Context<'a, H>,
}

struct PrimitiveLuauEnum<'a> {
    item_enum: &'a ItemEnum,
    ident_override: Option<String>,
}

struct LuauGenerics<'a>(&'a Generics);
struct LuauGenericArguments<'a, H> {
    value: &'a PathArguments,
    context: Context<'a, H>,
}

impl<'a, H> LuauGenericArguments<'a, H> {
    pub fn new(value: &'a PathArguments, context: Context<'a, H>) -> Self {
        Self { value, context }
    }
}

struct LuauStructField<'a, H> {
    value: &'a Field,
    context: Context<'a, H>,
}

impl<'a, H> LuauStructField<'a, H> {
    pub fn new(value: &'a Field, context: Context<'a, H>) -> Self {
        Self { value, context }
    }
}

struct LuauVariant<'a, H> {
    variant: &'a Variant,
    enum_ident: LuauIdent<'a>,
    /// `None` means untagged.
    tag: Option<&'a str>,
    context: Context<'a, H>,
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

struct LuauType<'a, H> {
    value: &'a Type,
    context: Context<'a, H>,
}

impl<'a, H> LuauType<'a, H> {
    fn new(value: &'a Type, context: Context<'a, H>) -> Self {
        Self { value, context }
    }
}

#[derive(Copy, Clone)]
struct LuauIdent<'a>(&'a Ident, Case);

#[derive(Copy, Clone)]
enum Case {
    Original,
    LowerCamelCase,
    UpperCamelCase,
    SnakeCase,
}

impl<'a, H> LuauFile<'a, H> {
    pub fn new(file: &'a File, hook: &'a H, foreign_files: &'a [File]) -> Self {
        Self {
            context: Context {
                file,
                hook,
                foreign_files,
            },
        }
    }
}

impl<'a, H: Hook> Display for LuauFile<'a, H> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str("local module = {}\n\n")?;
        line_breaked(|| {
            self.context
                .file
                .items
                .iter()
                .filter(|item| {
                    item_is_included(item, |ident| self.context.hook.include_type(ident))
                })
                .map(|item| LuauItem::new(item, self.context))
        })
        .fmt(f)?;
        f.write_str("return module\n")?;
        Ok(())
    }
}

fn item_is_included(item: &Item, include_ident: impl Fn(&str) -> bool) -> bool {
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

impl<'a, H: Hook> Display for LuauItem<'a, H> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match &self.value {
            Item::Enum(e) => LuauEnum::new(e, self.context).fmt(f)?,
            Item::Struct(s) => LuauStruct::new(s, self.context).fmt(f)?,
            Item::Type(_) => {}
            _ => {}
        }
        Ok(())
    }
}

impl<'a, H: Hook> Display for LuauStruct<'a, H> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let ident = LuauIdent(&self.value.ident, Case::Original);
        let generics = LuauGenerics(&self.value.generics);
        match &self.value.fields {
            Fields::Named(fields) => {
                let fields = line_breaked(|| {
                    fields
                        .named
                        .iter()
                        .map(|f| LuauStructField::new(f, self.context))
                });
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
                let ty = LuauType::new(&field.ty, self.context);
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

impl<'a, H: Hook> Display for LuauEnum<'a, H> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let args = self.value.attrs.iter().find_map(|attr| {
            if attr.path().segments.last()?.ident != "serde" {
                return None;
            }
            Some(SerdeTagArgs::from_meta(&attr.meta).expect("unknown serde attribute"))
        });
        match args {
            None => {
                // Primitive enum
                let primitive_enum = PrimitiveLuauEnum {
                    item_enum: self.value,
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
                            item_enum: self.value,
                            tag: None,
                            context: self.context,
                        };
                        rich_enum.fmt(f)?;
                    }
                    (Some(tag), _) => {
                        // Tagged enum
                        let rich_enum = RichLuauEnum {
                            item_enum: self.value,
                            tag: Some(&tag),
                            context: self.context,
                        };
                        rich_enum.fmt(f)?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl<'a, H: Hook> Display for RichLuauEnum<'a, H> {
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
                context: self.context,
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

impl<'a, H: Hook> Display for LuauGenericArguments<'a, H> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let PathArguments::AngleBracketed(args) = self.value else {
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
            LuauType::new(ty, self.context).fmt(f)?;
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

impl<'a, H: Hook> Display for LuauStructField<'a, H> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let ty = LuauType::new(&self.value.ty, self.context);
        let ident = self
            .value
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

impl<'a, H: Hook> Display for LuauVariant<'a, H> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let enum_ident = self.enum_ident;
        let variant_ident = LuauVariantIdent {
            enum_ident,
            variant: self.variant,
        };
        write!(f, "export type {variant_ident} = ")?;
        let upper_ident = LuauIdent(&self.variant.ident, Case::UpperCamelCase);
        match &self.variant.fields {
            Fields::Named(fields) => {
                // Struct-like enum variant
                f.write_str("{ ")?;
                if let Some(tag) = self.tag {
                    // Tagged enum
                    write!(f, r#"{tag}: "{upper_ident}", "#)?;
                }
                let fields = line_breaked(|| {
                    fields
                        .named
                        .iter()
                        .map(|f| LuauStructField::new(f, self.context))
                });
                fields.fmt(f)?;
                f.write_str("}")?;
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
                if let Some(tag) = self.tag {
                    // Tagged

                    // Ideally, we would just build a type intersection with the type of the single field.
                    // Example: ClipSlotDescriptor_ByIndex = {address: "ByIndex"} & playtime.SlotAddress
                    // write!(
                    //     f,
                    //     r#"{{ {tag}: "{upper_ident}" }} & {}"#,
                    //     LuauType::new(&field.ty, self.context)
                    // )?;
                    // However, Luau somehow doesn't understand this, so we have to inline the referenced type.
                    // I reported this bug in https://github.com/luau-lang/luau/issues/961.

                    // As an alternative, we resolve and inline the referenced types
                    let referenced_type_ident = match &field.ty {
                        Type::Path(p) => &p.path.segments.last().unwrap().ident,
                        _ => panic!("Enum variant {upper_ident} content doesn't contain type info"),
                    };
                    let referenced_item = self.context.find_item_by_ident(referenced_type_ident)
                        .unwrap_or_else(|| panic!("Couldn't find referred type of enum variant {enum_ident}::{upper_ident} (or is not a struct or enum)."));
                    match referenced_item {
                        Item::Struct(ItemStruct {
                            fields: Fields::Named(fields_named),
                            ..
                        }) => {
                            // The referenced item is a struct. Inline all struct fields.
                            f.write_str("{ ")?;
                            write!(f, r#"{tag}: "{upper_ident}", "#)?;
                            let fields = line_breaked(|| {
                                fields_named
                                    .named
                                    .iter()
                                    .map(|f| LuauStructField::new(f, self.context))
                            });
                            fields.fmt(f)?;
                            f.write_str("}")?;
                        }
                        Item::Enum(ItemEnum { variants, .. }) => {
                            // The referenced item is an enum. It must be an untagged one!
                            // We don't check this here. Example: BookmarkRef
                            // Create a tagged union of all variants.
                            for (i, variant) in variants.iter().enumerate() {
                                let Fields::Named(fields_named) = &variant.fields else {
                                    let luau_ident =
                                        LuauIdent(referenced_type_ident, Case::UpperCamelCase);
                                    panic!("Enum {luau_ident} referenced by {enum_ident}::{upper_ident} doesn't contain named fields. This is not supported at the moment.");
                                };
                                if i > 0 {
                                    f.write_str(" | ")?;
                                }
                                f.write_str("{ ")?;
                                write!(f, r#"{tag}: "{upper_ident}", "#)?;
                                let fields = line_breaked(|| {
                                    fields_named
                                        .named
                                        .iter()
                                        .map(|f| LuauStructField::new(f, self.context))
                                });
                                fields.fmt(f)?;
                                f.write_str("}")?;
                            }
                        }
                        _ => {}
                    }
                } else {
                    // Untagged
                    write!(f, "{}", LuauType::new(&field.ty, self.context))?;
                }
            }
            Fields::Unit => {
                // Primitive enum variant
                if let Some(tag) = self.tag {
                    // Tagged enum
                    write!(f, r#"{{ {tag}: "{upper_ident}" }}"#)?;
                } else {
                    panic!("untagged enums with primitive variants invalid");
                }
            }
        };
        Ok(())
    }
}

impl<'a, H: Hook> Display for LuauType<'a, H> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.value {
            Type::Path(p) => {
                let el = p.path.segments.last().unwrap();
                let ident = el.ident.to_string();
                match ident.as_str() {
                    "Option" => {
                        let ty = get_type_arg(&el.arguments, 0);
                        write!(f, "{}?", LuauType::new(ty, self.context))?;
                    }
                    "Vec" | "EnumSet" | "HashSet" => {
                        let ty = get_type_arg(&el.arguments, 0);
                        write!(f, "{{{}}}", LuauType::new(ty, self.context))?;
                    }
                    "HashMap" | "Map" => {
                        let ty1 = get_type_arg(&el.arguments, 0);
                        let ty2 = get_type_arg(&el.arguments, 1);
                        write!(
                            f,
                            "{{[{}]: {}}}",
                            LuauType::new(ty1, self.context),
                            LuauType::new(ty2, self.context)
                        )?;
                    }
                    _ => {
                        let final_type_name = translate_type_name(&ident).unwrap_or(&ident);
                        let generic_arguments =
                            LuauGenericArguments::new(&el.arguments, self.context);
                        if p.path.segments.len() > 1 {
                            if let Some(first_seg) = p.path.segments.first() {
                                if let Some(module) = self
                                    .context
                                    .hook
                                    .translate_crate_name(&first_seg.ident.to_string())
                                {
                                    write!(f, "{}.", module)?;
                                }
                            }
                        }
                        write!(f, "{final_type_name}{generic_arguments}")?;
                    }
                }
            }
            _ => panic!("unsupported type {:?}", self.value),
        }
        Ok(())
    }
}

fn translate_type_name(ident: &str) -> Option<&str> {
    let translated = match ident {
        "usize" | "u8" | "u16" | "u32" | "u64" | "isize" | "i8" | "i16" | "i32" | "i64"
        | "NonZeroU32" => "number",
        "f64" | "f32" => "number",
        "PathBuf" | "NaiveDateTime" | "Version" | "String" => "string",
        "bool" => "boolean",
        // serde_json::Value
        "Value" => "any",
        _ => return None,
    };
    Some(translated)
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
