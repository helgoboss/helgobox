use crate::domain::pot::{PluginId, ProductId};
use crate::domain::LimitedAsciiString;
use ascii::{AsciiString, ToAsciiChar};
use ini::Ini;
use std::collections::HashMap;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::path::Path;
use walkdir::WalkDir;

#[derive(Clone, Debug, Default)]
pub struct PluginDatabase {
    plugins: HashMap<PluginId, Plugin>,
    products: Vec<Product>,
}

#[derive(Default)]
struct ProductAccumulator {
    products: Vec<Product>,
}

impl ProductAccumulator {
    pub fn get_or_add_product(&mut self, name: &str, kind: Option<ProductKind>) -> ProductId {
        let existing_product = self
            .products
            .iter()
            .enumerate()
            .find(|(_, product)| product.name == name && product.kind == kind);
        let i = match existing_product {
            None => {
                let new_product = Product {
                    name: name.to_string(),
                    kind,
                };
                self.products.push(new_product);
                self.products.len() - 1
            }
            Some((i, _)) => i,
        };
        ProductId(i as u32)
    }

    pub fn into_products(self) -> Vec<Product> {
        self.products
    }
}

impl PluginDatabase {
    pub fn crawl(reaper_resource_dir: &Path) -> Self {
        let mut product_accumulator = ProductAccumulator::default();
        let plugins = crawl_plugins(&mut product_accumulator, reaper_resource_dir);
        Self {
            plugins: plugins.into_iter().map(|p| (p.common.id, p)).collect(),
            products: product_accumulator.into_products(),
        }
    }

    pub fn plugins(&self) -> impl Iterator<Item = &Plugin> {
        self.plugins.values()
    }

    pub fn find_plugin_by_id(&self, plugin_id: &PluginId) -> Option<&Plugin> {
        self.plugins.get(plugin_id)
    }

    pub fn find_product_by_id(&self, product_id: &ProductId) -> Option<&Product> {
        self.products.get(product_id.0 as usize)
    }
}

/// A product - an abstraction over related plug-ins.
///
/// Example: The product "Zebra2 (u-he)". This ignores architecture and plug-in framework.
/// So this product stands for all of: "Zebra VSTi", "Zebra VST3i", "Zebra CLAPi",
/// "Zebra VST (x86_64)", and so on.
#[derive(Clone, Debug)]
pub struct Product {
    pub name: String,
    pub kind: Option<ProductKind>,
}

/// TODO-high CONTINUE Also scan JS plug-ins!
#[derive(Clone, Debug)]
pub struct Plugin {
    /// Contains data relevant for all kinds of plug-ins.
    pub common: PluginCommon,
    /// Contains data specific to certain kinds of plug-ins.
    pub kind: PluginKind,
}

#[derive(Clone, Debug)]
pub struct PluginCommon {
    /// Uniquely identifies the plug-in in a rather cheap way (copyable).
    pub id: PluginId,
    /// Full name of the plug-in (for display purposes mainly).
    pub name: String,
    /// Whether we have an effect or an instrument or unknown.
    pub product_kind: Option<ProductKind>,
    /// What product this plug-in belongs to.
    pub product_id: ProductId,
}

impl Display for PluginCommon {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str(&self.id.kind_name())?;
        if let Some(ProductKind::Instrument) = self.product_kind {
            f.write_str("i")?;
        }
        write!(f, ": {}", &self.name)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum PluginKind {
    Vst(VstPlugin),
    Clap(ClapPlugin),
}

#[derive(Clone, Debug)]
pub struct ClapPlugin {
    /// Real file name, no characters replaced.
    pub file_name: String,
    /// Checksum looks different than in VST plug-in INI files.
    pub checksum: String,
    pub id: String,
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub enum VstPluginKind {
    Vst2 { magic_number: String },
    Vst3 { uid_hash: String, uid: String },
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

fn build_clap_plugin_id(id_expression: &str) -> Result<PluginId, &'static str> {
    let ascii_string: Result<AsciiString, _> =
        id_expression.chars().map(|c| c.to_ascii_char()).collect();
    let ascii_string = ascii_string.map_err(|_| "CLAP plug-in ID not ASCII")?;
    let id = PluginId::Clap {
        clap_id: LimitedAsciiString::try_from_ascii_str(&ascii_string)?,
    };
    Ok(id)
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

fn crawl_plugins(
    product_accumulator: &mut ProductAccumulator,
    reaper_resource_dir: &Path,
) -> Vec<Plugin> {
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
                PlugType::Vst => crawl_vst_plugins_in_ini_file(product_accumulator, ini),
                PlugType::Clap => crawl_clap_plugins_in_ini_file(product_accumulator, ini),
            };
            Some(plugins)
        })
        .flatten()
        .collect()
}

fn crawl_clap_plugins_in_ini_file(
    product_accumulator: &mut ProductAccumulator,
    ini: Ini,
) -> Vec<Plugin> {
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
                    let product_kind = match product_kind_id {
                        "0" => Some(ProductKind::Effect),
                        "1" => Some(ProductKind::Instrument),
                        _ => None,
                    };
                    let plugin = Plugin {
                        common: PluginCommon {
                            id: build_clap_plugin_id(key).ok()?,
                            name: plugin_name.to_string(),
                            product_kind,
                            product_id: product_accumulator
                                .get_or_add_product(plugin_name, product_kind),
                        },
                        kind: PluginKind::Clap(ClapPlugin {
                            file_name: file_name.to_string(),
                            checksum: checksum.to_string(),
                            id: key.to_string(),
                        }),
                    };
                    Some(plugin)
                })
                .collect();
            Some(plugins)
        })
        .flatten()
        .collect()
}

fn crawl_vst_plugins_in_ini_file(
    product_accumulator: &mut ProductAccumulator,
    ini: Ini,
) -> Vec<Plugin> {
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
                common: PluginCommon {
                    id: vst_kind.plugin_id().ok()?,
                    product_id: product_accumulator.get_or_add_product(name.as_str(), product_kind),
                    name,
                    product_kind,
                },
                kind: PluginKind::Vst(VstPlugin {
                    safe_file_name,
                    shell_qualifier,
                    checksum,
                    kind: vst_kind,
                }),
            };
            Some(plugin)
        })
        .collect()
}
