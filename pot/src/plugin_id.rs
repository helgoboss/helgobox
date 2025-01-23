use base::LimitedAsciiString;
use std::fmt;
use std::fmt::{Display, Formatter};

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum PluginId {
    Vst2 { vst_magic_number: i32 },
    Vst3 { vst_uid: [u32; 4] },
    Clap { clap_id: LimitedAsciiString<100> },
    Js { js_id: LimitedAsciiString<100> },
}

impl PluginId {
    pub fn vst2(vst_magic_number: i32) -> Self {
        Self::Vst2 { vst_magic_number }
    }

    pub fn vst3(vst_uid: [u32; 4]) -> Self {
        Self::Vst3 { vst_uid }
    }

    pub fn clap(id_expression: &str) -> Result<Self, &'static str> {
        let id = Self::Clap {
            clap_id: LimitedAsciiString::try_from_str(id_expression)?,
        };
        Ok(id)
    }

    pub fn js(id_expression: &str) -> Result<Self, &'static str> {
        // It's better to normalize to lowercase. When loading the FX via TrackFX_AddByName,
        // REAPER is case-insensitive anyway and we should be as well when doing comparisons.
        let lowercase_id_expression = id_expression.to_lowercase();
        let id = Self::Js {
            js_id: LimitedAsciiString::try_from_str(&lowercase_id_expression)?,
        };
        Ok(id)
    }

    pub fn parse_from_rxml_line(line: &str) -> Result<PluginId, &'static str> {
        let line = line.trim();
        let mut tokens = splitty::split_unquoted_whitespace(line).unwrap_quotes(true);
        let tag_opener = tokens.next().ok_or("missing FX tag opener")?;
        match tag_opener {
            "<VST" => {
                // Examples:
                // - <VST "VSTi: Zebra2 (u-he)" Zebra2.vst 0 Schmackes 1397572658<565354534D44327A6562726132000000> ""
                // - <VST "VST3i: Pianoteq 8 (Modartt) (1->5ch)" "Pianoteq 8.vst3" 0 "" 1031062328{565354507438717069616E6F74657120} ""
                // Skip plug-in name, file, zero, custom name
                for _ in 0..4 {
                    tokens.next();
                }
                // Process ID expression
                let id_expression = tokens.next().ok_or("missing VST ID expression")?;
                if let Some((_, remainder)) = id_expression.split_once('{') {
                    // VST3
                    let vst3_uid_string = remainder.strip_suffix('}').unwrap_or(remainder);
                    let uid = parse_vst3_uid(vst3_uid_string)?;
                    Ok(Self::vst3(uid))
                } else if let Some((magic_number_string, _)) = id_expression.split_once('<') {
                    // VST2
                    let magic_number = parse_vst2_magic_number(magic_number_string)?;
                    Ok(Self::vst2(magic_number))
                } else {
                    Err("couldn't process VST ID expression")
                }
            }
            "<CLAP" => {
                // Example: <CLAP "CLAPi: Surge XT (Surge Synth Team)" org.surge-synth-team.surge-xt Surgi
                // Skip plug-in name
                tokens.next();
                // Process ID expression
                let id_expression = tokens.next().ok_or("missing CLAP ID expression")?;
                Self::clap(id_expression)
            }
            "<JS" => {
                // Example: <JS analysis/hund ""
                let id_expression = tokens.next().ok_or("missing JS ID expression")?;
                Self::js(id_expression)
            }
            _ => Err("unknown FX tag opener"),
        }
    }

    pub fn kind(&self) -> PluginKind {
        match self {
            PluginId::Vst2 { .. } => PluginKind::Vst2,
            PluginId::Vst3 { .. } => PluginKind::Vst3,
            PluginId::Clap { .. } => PluginKind::Clap,
            PluginId::Js { .. } => PluginKind::Js,
        }
    }

    pub fn content_formatted_for_reaper(&self) -> String {
        PluginIdContentInReaperFormat(self).to_string()
    }
}

/// Example: `1967946098` for a VST2 plug-in ID.
pub struct PluginIdContentInReaperFormat<'a>(pub &'a PluginId);

impl Display for PluginIdContentInReaperFormat<'_> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.0 {
            PluginId::Clap { clap_id } => clap_id.fmt(f),
            PluginId::Js { js_id } => js_id.fmt(f),
            PluginId::Vst2 { vst_magic_number } => vst_magic_number.fmt(f),
            PluginId::Vst3 { vst_uid } => {
                // D39D5B69 D6AF42FA 12345678 534D4433
                write!(
                    f,
                    "{:X}{:X}{:X}{:X}",
                    vst_uid[0], vst_uid[1], vst_uid[2], vst_uid[3],
                )
            }
        }
    }
}

/// Example: `vst|1967946098` for a VST2 plug-in ID.
pub struct PluginIdInPipeFormat<'a>(pub &'a PluginId);

impl Display for PluginIdInPipeFormat<'_> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let kind = self.0.kind();
        let kind = kind.as_ref();
        let content = PluginIdContentInReaperFormat(self.0);
        write!(f, "{kind}|{content}")
    }
}

/// When adding a new variant, the serialization should correspond to the string which is used
/// as prefix for the ini file names in "REAPER_RESOURCE_PATH/presets".
#[derive(Copy, Clone, Eq, PartialEq, Debug, strum::AsRefStr, strum::EnumString)]
pub enum PluginKind {
    #[strum(serialize = "vst")]
    Vst2,
    #[strum(serialize = "vst3")]
    Vst3,
    #[strum(serialize = "clap")]
    Clap,
    #[strum(serialize = "js")]
    Js,
}

impl PluginKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Vst2 => "VST",
            Self::Vst3 => "VST3",
            Self::Clap => "CLAP",
            Self::Js => "JS",
        }
    }

    /// Need to put some random string in front of "<" due to bug in REAPER < 6.69,
    /// otherwise loading by VST2 magic number doesn't work.
    pub fn reaper_add_by_name_prefix_fix(&self) -> &'static str {
        match self {
            Self::Vst2 | Self::Vst3 => "i7zh34z",
            Self::Clap | Self::Js => "",
        }
    }

    pub fn formatted_for_reaper(&self) -> &'static str {
        match self {
            Self::Vst2 => "<",
            Self::Vst3 => "{",
            Self::Clap | Self::Js => "",
        }
    }
}

/// "1397572658" => 1397572658
pub fn parse_vst2_magic_number(expression: &str) -> Result<i32, &'static str> {
    expression
        .parse()
        .map_err(|_| "couldn't parse VST2 magic number")
}

/// "565354507438717069616E6F74657120" => [0x56535450, 0x74387170, 0x69616E6F, 0x74657120]
pub fn parse_vst3_uid(expression: &str) -> Result<[u32; 4], &'static str> {
    fn parse_component(text: &str, i: usize) -> Result<u32, &'static str> {
        let from = i * 8;
        let until = from + 8;
        let parsed = u32::from_str_radix(&text[from..until], 16)
            .map_err(|_| "couldn't parse VST3 uid component")?;
        Ok(parsed)
    }
    let uid = [
        parse_component(expression, 0)?,
        parse_component(expression, 1)?,
        parse_component(expression, 2)?,
        parse_component(expression, 3)?,
    ];
    Ok(uid)
}

#[cfg(test)]
mod tests {
    use crate::PluginId;

    #[test]
    pub fn vst2() {
        assert_eq!(
            PluginId::parse_from_rxml_line(
                r#"<VST "VSTi: Zebra2 (u-he)" Zebra2.vst 0 Schmackes 1397572658<565354534D44327A6562726132000000> """#
            ),
            Ok(PluginId::vst2(1397572658))
        );
    }

    #[test]
    pub fn vst3() {
        assert_eq!(
            PluginId::parse_from_rxml_line(
                r#"<VST "VST3i: Pianoteq 8 (Modartt) (1->5ch)" "Pianoteq 8.vst3" 0 "" 1031062328{565354507438717069616E6F74657120} """#
            ),
            Ok(PluginId::vst3([
                0x56535450, 0x74387170, 0x69616E6F, 0x74657120
            ]))
        );
    }

    #[test]
    pub fn clap() {
        assert_eq!(
            PluginId::parse_from_rxml_line(
                r#"<CLAP "CLAPi: Surge XT (Surge Synth Team)" org.surge-synth-team.surge-xt Surgi"#
            ),
            Ok(PluginId::clap("org.surge-synth-team.surge-xt").unwrap())
        );
    }

    #[test]
    pub fn js() {
        assert_eq!(
            PluginId::parse_from_rxml_line(r#"<JS Analysis/hund """#),
            Ok(PluginId::js("analysis/hund").unwrap())
        );
    }
}
