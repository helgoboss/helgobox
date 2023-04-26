use crate::domain::pot::PluginId;
use crate::domain::LimitedAsciiString;
use ascii::{AsciiString, ToAsciiChar};
use ini::Ini;
use std::path::Path;
use walkdir::WalkDir;

/// TODO-high CONTINUE Also scan JS plug-ins!
#[derive(Debug)]
pub struct Plugin {
    pub kind: PluginKind,
    pub product_kind: Option<ProductKind>,
    pub name: String,
}

#[derive(Debug)]
pub enum PluginKind {
    Vst(VstPlugin),
    Clap(ClapPlugin),
}

#[derive(Debug)]
pub struct ClapPlugin {
    pub file_name: String,
    pub checksum: String,
    pub id: String,
}

#[derive(Debug)]
pub struct VstPlugin {
    /// Safe means: Each space and special character is replaced with an underscore.
    pub safe_file_name: String,
    /// A value which identifies the actual plug-in within a shell file.
    ///
    /// Some files are just shells. That means, they contain multiple actual plug-ins.
    /// If this plug-in is part of a shell file, this value will be set. In practice, it's
    /// equal to the magic number / uid_hash.
    pub shell_qualifier: Option<String>,
    pub checksum: String,
    pub kind: VstPluginKind,
}

#[derive(Debug)]
pub enum VstPluginKind {
    Vst2 { magic_number: String },
    Vst3 { uid_hash: String, uid: String },
}

impl PluginKind {
    pub fn plugin_id(&self) -> Result<PluginId, &'static str> {
        match self {
            PluginKind::Vst(vst) => vst.kind.plugin_id(),
            PluginKind::Clap(clap) => clap.plugin_id(),
        }
    }
}

impl VstPluginKind {
    pub fn plugin_id(&self) -> Result<PluginId, &'static str> {
        let id = match self {
            VstPluginKind::Vst2 { magic_number } => PluginId::Vst2 {
                vst_magic_number: magic_number
                    .parse()
                    .map_err(|_| "couldn't parse VST2 magic number")?,
            },
            VstPluginKind::Vst3 { uid, .. } => {
                fn parse_component(text: &str, i: usize) -> Result<u32, &'static str> {
                    let from = i * 8;
                    let until = from + 8;
                    let parsed = u32::from_str_radix(&text[from..until], 16)
                        .map_err(|_| "couldn't parse VST3 uid component")?;
                    Ok(parsed)
                }
                PluginId::Vst3 {
                    vst_uid: [
                        parse_component(uid, 0)?,
                        parse_component(uid, 1)?,
                        parse_component(uid, 2)?,
                        parse_component(uid, 3)?,
                    ],
                }
            }
        };
        Ok(id)
    }
}

impl ClapPlugin {
    pub fn plugin_id(&self) -> Result<PluginId, &'static str> {
        let ascii_string: Result<AsciiString, _> =
            self.id.chars().map(|c| c.to_ascii_char()).collect();
        let ascii_string = ascii_string.map_err(|_| "CLAP plug-in ID not ASCII")?;
        let id = PluginId::Clap {
            clap_id: LimitedAsciiString::try_from_ascii_str(&ascii_string)?,
        };
        Ok(id)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ProductKind {
    Effect,
    Instrument,
    Loop,
    OneShot,
}

impl ProductKind {
    pub fn komplete_id(&self) -> &'static u32 {
        const INSTRUMENT: u32 = 1;
        const EFFECT: u32 = 2;
        const LOOP: u32 = 4;
        const ONE_SHOT: u32 = 8;
        use ProductKind::*;
        match self {
            Effect => &EFFECT,
            Instrument => &INSTRUMENT,
            Loop => &LOOP,
            OneShot => &ONE_SHOT,
        }
    }
}

pub fn crawl_plugins(reaper_resource_dir: &Path) -> Vec<Plugin> {
    WalkDir::new(reaper_resource_dir)
        .max_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if !entry.file_type().is_file() {
                return None;
            }
            let file_name = entry.file_name().to_str()?;
            enum PlugType {
                Vst,
                Clap,
            }
            let plug_type = if file_name.starts_with("reaper-vstplugins") {
                PlugType::Vst
            } else if file_name.starts_with("reaper-clap-") {
                PlugType::Clap
            } else {
                return None;
            };
            let ini = Ini::load_from_file(entry.path()).ok()?;
            let plugins = match plug_type {
                PlugType::Vst => crawl_vst_plugins_in_ini_file(ini),
                PlugType::Clap => crawl_clap_plugins_in_ini_file(ini),
            };
            Some(plugins)
        })
        .flatten()
        .collect()
}

fn crawl_clap_plugins_in_ini_file(ini: Ini) -> Vec<Plugin> {
    ini.iter()
        .filter_map(|(section, props)| {
            let file_name = section?;
            let checksum = props.get("_")?;
            let plugins: Vec<_> = props
                .iter()
                .filter_map(|(key, value)| {
                    if key == "_" {
                        return None;
                    }
                    let (product_kind_id, plugin_name) = value.split_once('|')?;
                    let plugin = Plugin {
                        kind: PluginKind::Clap(ClapPlugin {
                            file_name: file_name.to_string(),
                            checksum: checksum.to_string(),
                            id: key.to_string(),
                        }),
                        product_kind: match product_kind_id {
                            "0" => Some(ProductKind::Effect),
                            "1" => Some(ProductKind::Instrument),
                            _ => None,
                        },
                        name: plugin_name.to_string(),
                    };
                    Some(plugin)
                })
                .collect();
            Some(plugins)
        })
        .flatten()
        .collect()
}

fn crawl_vst_plugins_in_ini_file(ini: Ini) -> Vec<Plugin> {
    let Some(section) = ini.section(Some("vstcache")) else {
        return vec![];
    };
    section
        .iter()
        .filter_map(|(key, value)| {
            let (safe_file_name, shell_qualifier) = match key.split_once('<') {
                Some((first, second)) => (first.to_string(), Some(second.to_string())),
                None => (key.to_string(), None),
            };
            let mut value_iter = value.splitn(3, ',');
            let checksum = value_iter.next()?.to_string();
            let plugin_id_expression = value_iter.next()?;
            if plugin_id_expression == "0" {
                // Must be a plug-in shell, not an actual plug-in.
                return None;
            }
            let plugin_name_kind_expression = value_iter.next()?;
            let vst_kind = match plugin_id_expression.split_once('{') {
                None => VstPluginKind::Vst2 {
                    magic_number: plugin_id_expression.to_string(),
                },
                Some((left, right)) => VstPluginKind::Vst3 {
                    uid_hash: left.to_string(),
                    uid: right.to_string(),
                },
            };
            let (name, product_kind) = match plugin_name_kind_expression.split_once("!!!") {
                None => (
                    plugin_name_kind_expression.to_string(),
                    Some(ProductKind::Effect),
                ),
                Some((left, right)) => {
                    let kind = match right {
                        "VSTi" => Some(ProductKind::Instrument),
                        _ => None,
                    };
                    (left.to_string(), kind)
                }
            };
            let plugin = Plugin {
                kind: PluginKind::Vst(VstPlugin {
                    safe_file_name,
                    shell_qualifier,
                    checksum,
                    kind: vst_kind,
                }),
                product_kind,
                name,
            };
            Some(plugin)
        })
        .collect()
}
