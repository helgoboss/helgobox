#![allow(non_camel_case_types)]
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use std::ops::Add;

pub type Caption = &'static str;

pub struct ResourceInfo {
    y_scale: f64,
    height_scale: f64,
    named_ids: Vec<Id>,
}

/// Formats the info as C header file.
///
/// Useful if you want to preview the dialogs in Visual Studio.
pub struct ResourceInfoAsCHeaderCode<'a>(pub &'a ResourceInfo);

impl<'a> Display for ResourceInfoAsCHeaderCode<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for id in &self.0.named_ids {
            writeln!(f, "#define {} {}", id.name, id.value)?;
        }
        Ok(())
    }
}

/// Formats the header as Rust code.
///
/// Uses a similar format like bindgen because previously, bindgen was used to translate
/// the C header file to Rust.
pub struct ResourceInfoAsRustCode<'a>(pub &'a ResourceInfo);

impl<'a> Display for ResourceInfoAsRustCode<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("pub mod root {\n")?;
        writeln!(f, "    pub const Y_SCALE: f64 = {:.4};", self.0.y_scale)?;
        writeln!(
            f,
            "    pub const HEIGHT_SCALE: f64 = {:.4};",
            self.0.height_scale
        )?;
        for id in &self.0.named_ids {
            writeln!(f, "    pub const {}: u32 = {};", id.name, id.value)?;
        }
        f.write_str("}\n")?;
        Ok(())
    }
}

#[derive(Default)]
pub struct Resource {
    pub dialogs: Vec<Dialog>,
}

impl Resource {
    pub fn generate_info(&self, context: &Context) -> ResourceInfo {
        ResourceInfo {
            y_scale: context.y_scale,
            height_scale: context.height_scale,
            named_ids: self.named_ids().collect(),
        }
    }

    fn named_ids(&self) -> impl Iterator<Item = Id> + '_ {
        self.dialogs.iter().flat_map(|dialog| {
            fn get_if_named(id: Id) -> Option<Id> {
                if id.is_named() {
                    Some(id)
                } else {
                    None
                }
            }
            let named_dialog_id = get_if_named(dialog.id);
            let named_control_ids = dialog
                .controls
                .iter()
                .flat_map(|control| get_if_named(control.id));
            named_dialog_id.into_iter().chain(named_control_ids)
        })
    }
}

impl Display for Resource {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        for (i, dialog) in self.dialogs.iter().enumerate() {
            dialog.fmt(f)?;
            if i < self.dialogs.len() - 1 {
                f.write_str("\n\n")?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct Dialog {
    pub id: Id,
    pub rect: Rect,
    pub kind: DialogKind,
    pub styles: Styles,
    pub ex_styles: Styles,
    pub caption: Caption,
    pub font: Option<Font>,
    pub controls: Vec<Control>,
}

#[derive(Clone, Default)]
pub struct Styles(pub Vec<Style>);

impl Display for Styles {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        for (i, style) in self.0.iter().enumerate() {
            style.fmt(f)?;
            if i < self.0.len() - 1 {
                f.write_str(" | ")?;
            }
        }
        Ok(())
    }
}

#[derive(Copy, Clone, Default)]
pub struct Id {
    value: u32,
    name: &'static str,
}

impl Id {
    fn is_named(&self) -> bool {
        !self.name.is_empty()
    }
}

pub struct Context {
    pub next_id_value: u32,
    pub default_dialog: Dialog,
    pub y_scale: f64,
    pub height_scale: f64,
}

impl Context {
    pub fn default_dialog(&self) -> Dialog {
        self.default_dialog.clone()
    }

    pub fn rect(&self, x: u32, y: u32, width: u32, height: u32) -> Rect {
        self.rect_flexible(Rect::new(x, y, width, height))
    }

    pub fn rect_flexible(&self, rect: Rect) -> Rect {
        Rect {
            x: rect.x,
            y: scale(self.y_scale, rect.y),
            width: rect.width,
            height: scale(self.height_scale, rect.height),
        }
    }

    pub fn id(&mut self) -> Id {
        Id {
            value: self.next_id_value(),
            name: "",
        }
    }

    pub fn named_id(&mut self, name: &'static str) -> Id {
        Id {
            value: self.next_id_value(),
            name,
        }
    }

    fn next_id_value(&mut self) -> u32 {
        let v = self.next_id_value;
        self.next_id_value += 1;
        v
    }
}

fn scale(scale: f64, value: u32) -> u32 {
    (scale * value as f64).round() as _
}

impl Display for Id {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if self.name.is_empty() {
            self.value.fmt(f)
        } else {
            self.name.fmt(f)
        }
    }
}

#[derive(Copy, Clone, derive_more::Display)]
pub enum DialogKind {
    DIALOG,
    DIALOGEX,
}

impl Default for DialogKind {
    fn default() -> Self {
        Self::DIALOG
    }
}

#[derive(Clone, Default)]
pub struct Control {
    pub id: Id,
    /// Unlike in dialog, it's important to distinguish between Some and None because some
    /// controls need an empty string.
    pub caption: Option<Caption>,
    pub kind: ControlKind,
    pub sub_kind: Option<SubControlKind>,
    pub rect: Rect,
    pub styles: Styles,
}

impl Add<Style> for Control {
    type Output = Control;

    fn add(mut self, rhs: Style) -> Self::Output {
        self.styles.0.push(rhs);
        self
    }
}

struct Quoted<D>(D);

impl<D: Display> Display for Quoted<D> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "\"{}\"", self.0)
    }
}

struct LineBreaksEscaped<D>(D);

impl<D: Display> Display for LineBreaksEscaped<D> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.0
            .to_string()
            .replace("\r\n", "\\r\\n")
            .replace("\n", "\\r\\n")
            .fmt(f)
    }
}

fn opt<T: Display>(v: &Option<T>) -> Option<String> {
    let v = v.as_ref()?;
    Some(v.to_string())
}

fn req<T: Display>(v: T) -> Option<String> {
    Some(v.to_string())
}

impl Display for Dialog {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        writeln!(f, "{} {} {}", self.id, self.kind, self.rect)?;
        if !self.styles.0.is_empty() {
            writeln!(f, "STYLE {}", self.styles)?;
        }
        if !self.ex_styles.0.is_empty() {
            writeln!(f, "EXSTYLE {}", self.ex_styles)?;
        }
        if !self.caption.is_empty() {
            writeln!(f, "CAPTION {}", Quoted(self.caption))?;
        }
        if let Some(font) = self.font.as_ref() {
            writeln!(f, "FONT {}", font)?;
        }
        if !self.controls.is_empty() {
            f.write_str("BEGIN\n")?;
            for control in &self.controls {
                writeln!(f, "    {}", control)?;
            }
            f.write_str("END")?;
        }
        Ok(())
    }
}

impl Display for Control {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let caption = opt(&self.caption.map(LineBreaksEscaped).map(Quoted));
        let id = req(&self.id);
        let rect = req(&self.rect);
        let styles = if self.styles.0.is_empty() {
            None
        } else {
            Some(self.styles.to_string())
        };
        let args = if self.kind == ControlKind::CONTROL {
            vec![
                caption,
                id,
                req(Quoted(self.sub_kind.unwrap())),
                styles,
                rect,
            ]
        } else {
            vec![caption, id, rect, styles]
        };
        let args: Vec<_> = args.into_iter().flatten().collect();
        write!(f, "{} {}", self.kind, args.join(","))
    }
}

#[derive(Copy, Clone, PartialEq, derive_more::Display)]
pub enum ControlKind {
    LTEXT,
    RTEXT,
    COMBOBOX,
    PUSHBUTTON,
    CONTROL,
    EDITTEXT,
    GROUPBOX,
    DEFPUSHBUTTON,
    CTEXT,
}

impl Default for ControlKind {
    fn default() -> Self {
        Self::CTEXT
    }
}

#[derive(Copy, Clone, derive_more::Display)]
pub enum SubControlKind {
    Button,
    Static,
    msctls_trackbar32,
}

#[derive(Clone, Copy)]
pub struct Font {
    pub name: &'static str,
    pub size: u32,
}

impl Display for Font {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}, {}", self.size, Quoted(self.name))
    }
}

#[derive(Copy, Clone)]
pub struct Point(pub u32, pub u32);

impl Point {
    pub fn with_dimensions(&self, dimensions: Dimensions) -> Rect {
        Rect {
            x: self.0,
            y: self.1,
            width: dimensions.0,
            height: dimensions.1,
        }
    }
}

#[derive(Copy, Clone)]
pub struct Dimensions(pub u32, pub u32);

#[derive(Copy, Clone, Default)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Display for Rect {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}, {}, {}, {}", self.x, self.y, self.width, self.height)
    }
}

impl Rect {
    fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

pub fn pushbutton(caption: Caption, id: Id, rect: Rect) -> Control {
    Control {
        id,
        caption: Some(caption),
        kind: ControlKind::PUSHBUTTON,
        rect,
        ..Default::default()
    }
}

pub fn groupbox(caption: Caption, id: Id, rect: Rect) -> Control {
    Control {
        id,
        caption: Some(caption),
        kind: ControlKind::GROUPBOX,
        rect,
        ..Default::default()
    }
}

pub fn defpushbutton(caption: Caption, id: Id, rect: Rect) -> Control {
    Control {
        id,
        caption: Some(caption),
        kind: ControlKind::DEFPUSHBUTTON,
        rect,
        ..Default::default()
    }
}

pub fn ltext(caption: Caption, id: Id, rect: Rect) -> Control {
    Control {
        id,
        caption: Some(caption),
        kind: ControlKind::LTEXT,
        rect,
        ..Default::default()
    }
}

pub fn rtext(caption: Caption, id: Id, rect: Rect) -> Control {
    Control {
        id,
        caption: Some(caption),
        kind: ControlKind::RTEXT,
        rect,
        ..Default::default()
    }
}

pub fn ctext(caption: Caption, id: Id, rect: Rect) -> Control {
    Control {
        id,
        caption: Some(caption),
        kind: ControlKind::CTEXT,
        rect,
        ..Default::default()
    }
}

pub fn combobox(id: Id, rect: Rect) -> Control {
    Control {
        id,
        kind: ControlKind::COMBOBOX,
        rect,
        ..Default::default()
    }
}

pub fn edittext(id: Id, rect: Rect) -> Control {
    Control {
        id,
        kind: ControlKind::EDITTEXT,
        rect,
        ..Default::default()
    }
}

pub fn control(caption: Caption, id: Id, sub_kind: SubControlKind, rect: Rect) -> Control {
    Control {
        id,
        caption: Some(caption),
        kind: ControlKind::CONTROL,
        sub_kind: Some(sub_kind),
        rect,
        ..Default::default()
    }
}

#[derive(Copy, Clone, derive_more::Display)]
pub enum Style {
    DS_SETFONT,
    DS_MODALFRAME,
    DS_3DLOOK,
    DS_FIXEDSYS,
    DS_CENTER,
    WS_POPUP,
    WS_VISIBLE,
    WS_CAPTION,
    WS_SYSMENU,
    DS_CONTROL,
    WS_CHILD,
    CBS_DROPDOWNLIST,
    CBS_HASSTRINGS,
    CBS_SORT,
    ES_MULTILINE,
    ES_READONLY,
    ES_WANTRETURN,
    WS_VSCROLL,
    WS_TABSTOP,
    WS_GROUP,
    WS_DISABLED,
    BS_AUTOCHECKBOX,
    BS_AUTORADIOBUTTON,
    TBS_BOTH,
    TBS_NOTICKS,
    SS_ETCHEDHORZ,
    SS_LEFTNOWORDWRAP,
    ES_AUTOHSCROLL,
    SS_CENTERIMAGE,
    SS_WORDELLIPSIS,
    // With negation
    #[display(fmt = "NOT WS_TABSTOP")]
    NOT_WS_TABSTOP,
    #[display(fmt = "NOT WS_GROUP")]
    NOT_WS_GROUP,
    // Ex styles
    WS_EX_TOPMOST,
    WS_EX_WINDOWEDGE,
}
