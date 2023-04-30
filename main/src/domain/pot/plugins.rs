use crate::base::file_util;
use crate::domain::pot::{parse_vst2_magic_number, parse_vst3_uid, PluginId, ProductId};
use ini::Ini;
use regex::Match;
use std::collections::HashMap;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{BufRead, BufReader};
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
    pub fn add_js_product(&mut self, name: String, kind: Option<ProductKind>) -> ProductId {
        let product = Product { name, kind };
        self.products.push(product);
        ProductId(self.products.len() as u32 - 1)
    }

    pub fn get_or_add_shared_library_product(
        &mut self,
        name_expression: &str,
        kind: Option<ProductKind>,
    ) -> ProductId {
        let name = if let Some(name) = ProductName::parse(name_expression) {
            name.main
        } else {
            name_expression
        };
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
        let shared_library_plugins =
            crawl_shared_library_plugins(&mut product_accumulator, reaper_resource_dir);
        let js_root_dir = reaper_resource_dir.join("Effects");
        let js_plugins = crawl_js_plugins(&mut product_accumulator, &js_root_dir);
        let plugin_map = shared_library_plugins
            .into_iter()
            .chain(js_plugins.into_iter())
            .map(|p| (p.common.core.id, p))
            .collect();
        Self {
            plugins: plugin_map,
            products: product_accumulator.into_products(),
        }
    }

    pub fn plugins(&self) -> impl Iterator<Item = &Plugin> {
        self.plugins.values()
    }

    pub fn products(&self) -> impl Iterator<Item = (ProductId, &Product)> {
        self.products
            .iter()
            .enumerate()
            .map(|(i, p)| (ProductId(i as _), p))
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

#[derive(Clone, Debug)]
pub struct Plugin {
    /// Contains data relevant for all kinds of plug-ins.
    pub common: PluginCommon,
    /// Contains data specific to certain kinds of plug-ins.
    pub kind: PluginKind,
}

#[derive(Clone, Debug)]
pub struct PluginCommon {
    /// Full name of the plug-in (for display purposes mainly).
    ///
    /// E.g. "Zebra2 (u-he)"
    pub name: String,
    pub core: PluginCore,
}

#[derive(Clone, Debug)]
pub struct PluginCore {
    /// Uniquely identifies the plug-in in a rather cheap way (copyable).
    pub id: PluginId,
    /// Whether we have an effect or an instrument or unknown.
    pub product_kind: Option<ProductKind>,
    /// What product this plug-in belongs to.
    pub product_id: ProductId,
}

impl Display for PluginCommon {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str(&self.core.id.kind_name())?;
        if let Some(ProductKind::Instrument) = self.core.product_kind {
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
    Js(JsPlugin),
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
pub struct JsPlugin {
    /// Relative path from JS root dir.
    pub path: String,
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
                vst_magic_number: parse_vst2_magic_number(magic_number)?,
            },
            VstPluginKind::Vst3 { uid, .. } => PluginId::Vst3 {
                vst_uid: parse_vst3_uid(uid)?,
            },
        };
        Ok(id)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, derive_more::Display)]
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
fn crawl_js_plugins(
    product_accumulator: &mut ProductAccumulator,
    js_root_dir: &Path,
) -> Vec<Plugin> {
    WalkDir::new(js_root_dir)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| !file_util::is_hidden(e))
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if !entry.file_type().is_file() {
                return None;
            }
            let relative_path = entry.path().strip_prefix(js_root_dir).ok()?;
            let relative_path = relative_path.to_str()?;
            let product_kind = Some(ProductKind::Effect);
            let js_desc = read_js_desc_from_file(entry.path())?;
            let plugin = Plugin {
                common: PluginCommon {
                    name: js_desc.clone(),
                    core: PluginCore {
                        id: PluginId::js(relative_path).ok()?,
                        product_kind,
                        product_id: product_accumulator
                            .add_js_product(js_desc.to_string(), product_kind),
                    },
                },
                kind: PluginKind::Js(JsPlugin {
                    path: relative_path.to_string(),
                }),
            };
            Some(plugin)
        })
        .collect()
}

fn crawl_shared_library_plugins(
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
                            core: PluginCore {
                                id: PluginId::clap(key).ok()?,
                                product_kind,
                                product_id: product_accumulator
                                    .get_or_add_shared_library_product(plugin_name, product_kind),
                            },
                            name: plugin_name.to_string(),
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
                    core: PluginCore {
                        id: vst_kind.plugin_id().ok()?,
                        product_id: product_accumulator
                            .get_or_add_shared_library_product(name.as_str(), product_kind),
                        product_kind,
                    },
                    name,
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

#[derive(Eq, PartialEq, Debug)]
struct ProductName<'a> {
    main: &'a str,
    arch: Option<&'a str>,
    company: &'a str,
    channels: Option<&'a str>,
}

impl<'a> ProductName<'a> {
    pub fn parse(name_expression: &'a str) -> Option<Self> {
        let four_part_regex = regex!(r#"(.*) \((.*)\) \((.*)\) \((.*)\)"#);
        let three_part_regex = regex!(r#"(.*) \((.*)\) \((.*)\)"#);
        let two_part_regex = regex!(r#"(.*) \((.*)\)"#);

        if let Some(captures) = four_part_regex.captures(name_expression) {
            return Some(ProductName {
                main: s(captures.get(1)),
                arch: Some(s(captures.get(2))),
                company: s(captures.get(3)),
                channels: Some(s(captures.get(4))),
            });
        }
        if let Some(captures) = three_part_regex.captures(name_expression) {
            let name = if &captures[2] == "x86_64" {
                ProductName {
                    main: s(captures.get(1)),
                    arch: Some(s(captures.get(2))),
                    company: s(captures.get(3)),
                    channels: None,
                }
            } else {
                ProductName {
                    main: s(captures.get(1)),
                    arch: None,
                    company: s(captures.get(2)),
                    channels: Some(s(captures.get(3))),
                }
            };
            return Some(name);
        }
        if let Some(captures) = two_part_regex.captures(name_expression) {
            return Some(ProductName {
                main: s(captures.get(1)),
                arch: None,
                company: s(captures.get(2)),
                channels: None,
            });
        }
        None
    }
}

fn s(m: Option<Match>) -> &str {
    m.unwrap().as_str()
}

fn read_js_desc_from_file(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut buffer = String::new();
    let mut reader = BufReader::new(&file);
    while let Ok(count) = reader.read_line(&mut buffer) {
        if count == 0 {
            // EOF
            break;
        }
        let line = buffer.trim();
        if let Some((left, right)) = line.split_once(":") {
            if left == "desc" {
                return Some(right.trim().to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::domain::pot::plugins::ProductName;

    #[test]
    pub fn product_name_parsing_2() {
        assert_eq!(
            ProductName::parse("TDR Nova (Tokyo Dawn Labs)"),
            Some(ProductName {
                main: "TDR Nova",
                arch: None,
                company: "Tokyo Dawn Labs",
                channels: None,
            })
        );
    }

    #[test]
    pub fn product_name_parsing_3_arch() {
        assert_eq!(
            ProductName::parse("VC 76 (x86_64) (Native Instruments GmbH)"),
            Some(ProductName {
                main: "VC 76",
                arch: Some("x86_64"),
                company: "Native Instruments GmbH",
                channels: None,
            })
        );
    }

    #[test]
    pub fn product_name_parsing_3_ch() {
        assert_eq!(
            ProductName::parse("Surge XT (Surge Synth Team) (2->6ch)"),
            Some(ProductName {
                main: "Surge XT",
                arch: None,
                company: "Surge Synth Team",
                channels: Some("2->6ch"),
            })
        );
    }

    #[test]
    pub fn product_name_parsing_4() {
        assert_eq!(
            ProductName::parse("Sitala (x86_64) (Decomposer) (32 out)"),
            Some(ProductName {
                main: "Sitala",
                arch: Some("x86_64"),
                company: "Decomposer",
                channels: Some("32 out"),
            })
        );
    }
}
