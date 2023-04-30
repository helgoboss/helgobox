use crate::domain::LimitedAsciiString;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum PluginId {
    Vst2 { vst_magic_number: u32 },
    Vst3 { vst_uid: [u32; 4] },
    Clap { clap_id: LimitedAsciiString<64> },
    Js { js_id: LimitedAsciiString<64> },
}

impl PluginId {
    pub fn vst2(vst_magic_number: u32) -> Self {
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
        let id = Self::Js {
            js_id: LimitedAsciiString::try_from_str(id_expression)?,
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

    pub fn kind_name(&self) -> &'static str {
        match self {
            PluginId::Vst2 { .. } => "VST",
            PluginId::Vst3 { .. } => "VST3",
            PluginId::Clap { .. } => "CLAP",
            PluginId::Js { .. } => "JS",
        }
    }

    /// Need to put some random string in front of "<" due to bug in REAPER < 6.69,
    /// otherwise loading by VST2 magic number doesn't work.
    pub fn add_by_name_prefix_fix(&self) -> &'static str {
        match self {
            PluginId::Vst2 { .. } | PluginId::Vst3 { .. } => "i7zh34z",
            PluginId::Clap { .. } | PluginId::Js { .. } => "",
        }
    }

    pub fn reaper_prefix(&self) -> &'static str {
        match self {
            PluginId::Vst2 { .. } => "<",
            PluginId::Vst3 { .. } => "{",
            PluginId::Clap { .. } | PluginId::Js { .. } => "",
        }
    }

    pub fn formatted_for_reaper(&self) -> String {
        match self {
            PluginId::Clap { clap_id } => clap_id.to_string(),
            PluginId::Js { js_id } => js_id.to_string(),
            PluginId::Vst2 { vst_magic_number } => vst_magic_number.to_string(),
            PluginId::Vst3 { vst_uid } => {
                // D39D5B69 D6AF42FA 12345678 534D4433
                format!(
                    "{:X}{:X}{:X}{:X}",
                    vst_uid[0], vst_uid[1], vst_uid[2], vst_uid[3],
                )
            }
        }
    }

    pub fn simple_kind(&self) -> SimplePluginKind {
        match self {
            PluginId::Vst2 { .. } => SimplePluginKind::Vst2,
            PluginId::Vst3 { .. } => SimplePluginKind::Vst3,
            PluginId::Clap { .. } => SimplePluginKind::Clap,
            PluginId::Js { .. } => SimplePluginKind::Js,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SimplePluginKind {
    Vst2,
    Vst3,
    Clap,
    Js,
}

/// "1397572658" => 1397572658
pub fn parse_vst2_magic_number(expression: &str) -> Result<u32, &'static str> {
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
    use crate::domain::pot::PluginId;

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
            PluginId::parse_from_rxml_line(r#"<JS analysis/hund """#),
            Ok(PluginId::js("analysis/hund").unwrap())
        );
    }
}
