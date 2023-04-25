use crate::domain::pot::PluginId;
use ini::Ini;
use std::path::Path;

#[derive(Debug)]
pub struct Plugin {
    /// Safe means: Each space and special character is replaced with an underscore.
    pub safe_file_name: String,
    /// A value which identifies the actual plug-in within a shell file.
    ///
    /// Some files are just shells. That means, they contain multiple actual plug-ins.
    /// If this plug-in is part of a shell file, this value will be set. In practice, it's
    /// equal to the magic number / uid_hash.
    pub shell_qualifier: Option<String>,
    pub checksum: String,
    pub kind: PluginKind,
    pub product_kind: ProductKind,
    pub name: String,
}

#[derive(Debug)]
pub enum PluginKind {
    Vst2 { magic_number: String },
    Vst3 { uid_hash: String, uid: String },
}

impl PluginKind {
    pub fn plugin_id(&self) -> Result<PluginId, &'static str> {
        let id = match self {
            PluginKind::Vst2 { magic_number } => PluginId::Vst2 {
                vst_magic_number: magic_number
                    .parse()
                    .map_err(|_| "couldn't parse VST2 magic number")?,
            },
            PluginKind::Vst3 { uid, .. } => {
                fn parse_component(text: &str, i: usize) -> Result<u32, &'static str> {
                    let from = i * 8;
                    let until = from + 8;
                    let parsed = u32::from_str_radix(&text[from..until], 16)
                        .map_err(|_| "couldn't pare VST3 uid component")?;
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

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ProductKind {
    Effect,
    Instrument,
    Loop,
    OneShot,
    Other,
}

pub fn crawl_plugins(reaper_resource_dir: &Path) -> Vec<Plugin> {
    let file_names = [
        "reaper-vstplugins.ini",
        "reaper-vstplugins64.ini",
        "reaper-vstplugins_arm64.ini",
    ];
    file_names
        .iter()
        .flat_map(|file_name| {
            let file_path = reaper_resource_dir.join(file_name);
            crawl_plugins_in_ini_file(&file_path).into_iter()
        })
        .collect()
}

fn crawl_plugins_in_ini_file(path: &Path) -> Vec<Plugin> {
    let Ok(ini) = Ini::load_from_file(path) else {
        return vec![];
    };
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
            let plugin_name_kind_expression = value_iter.next()?;
            let kind = match plugin_id_expression.split_once('{') {
                None => PluginKind::Vst2 {
                    magic_number: plugin_id_expression.to_string(),
                },
                Some((left, right)) => PluginKind::Vst3 {
                    uid_hash: left.to_string(),
                    uid: right.to_string(),
                },
            };
            let (name, product_kind) = match plugin_name_kind_expression.split_once("!!!") {
                None => (plugin_name_kind_expression.to_string(), ProductKind::Effect),
                Some((left, right)) => {
                    let kind = match right {
                        "VSTi" => ProductKind::Instrument,
                        _ => ProductKind::Other,
                    };
                    (left.to_string(), kind)
                }
            };
            let plugin = Plugin {
                safe_file_name,
                shell_qualifier,
                checksum,
                kind,
                product_kind,
                name,
            };
            Some(plugin)
        })
        .collect()
}
