use helgobox_api::persistence::{
    AbsoluteMode, ActionInvocationKind, ButtonFilter, Compartment, Glue, Interval, Mapping,
    ReaperActionTarget, ReaperCommand, Source, StreamDeckButtonBackground, StreamDeckButtonDesign,
    StreamDeckButtonFadingImageForeground, StreamDeckButtonForeground,
    StreamDeckButtonImageBackground, StreamDeckSource, Target,
};
use reaper_high::{ActionCharacter, Reaper};
use reaper_medium::{CommandItem, MenuOrToolbarItem};

pub fn create_stream_deck_compartment_reflecting_toolbar(
    toolbar_name: &str,
) -> anyhow::Result<Compartment> {
    let items = (0..)
        .map(|pos| get_toolbar_cmd_item(toolbar_name, pos))
        .take_while(|item| item.is_some())
        .flatten()
        .flatten();
    let reaper_resource_path = Reaper::get().resource_path();
    let button_mappings = items.enumerate().map(|(i, item)| {
        let action = Reaper::get()
            .main_section()
            .action_by_command_id(item.command_id);
        let action_character = action.character().unwrap_or(ActionCharacter::Trigger);
        let button_icon_path = item
            .icon_file_name
            .as_ref()
            .map(|icon| format!("Data/toolbar_icons/200/{icon}"))
            // It's possible that the image doesn't exist (e.g. if it's an internal icon)
            .filter(|path| reaper_resource_path.join(path).exists());
        let static_text = if item.icon_file_name.is_some() {
            String::new()
        } else {
            item.label.replace(' ', "\n").clone()
        };
        let button_design = match action_character {
            ActionCharacter::Toggle => StreamDeckButtonDesign {
                foreground: match button_icon_path {
                    None => StreamDeckButtonForeground::FadingColor(Default::default()),
                    Some(path) => {
                        let fg = StreamDeckButtonFadingImageForeground { path };
                        StreamDeckButtonForeground::FadingImage(fg)
                    }
                },
                static_text,
                ..Default::default()
            },
            ActionCharacter::Trigger => StreamDeckButtonDesign {
                background: button_icon_path
                    .map(|path| {
                        let fg = StreamDeckButtonImageBackground { path };
                        StreamDeckButtonBackground::Image(fg)
                    })
                    .unwrap_or_default(),
                static_text,
                ..Default::default()
            },
        };
        let source = StreamDeckSource {
            button_index: i as u32,
            button_design,
        };
        let glue = match action_character {
            ActionCharacter::Toggle => Glue {
                source_interval: if item.icon_file_name.is_some() {
                    // Show icon dimmed when off and with full brightness when on
                    Some(Interval(0.5, 1.0))
                } else {
                    None
                },
                absolute_mode: Some(AbsoluteMode::ToggleButton),
                ..Default::default()
            },
            ActionCharacter::Trigger => Glue {
                button_filter: Some(ButtonFilter::PressOnly),
                ..Default::default()
            },
        };
        let target = ReaperActionTarget {
            command: {
                let cmd = match action.command_name() {
                    // Built-in actions don't have a command name but a persistent command ID.
                    // Use command ID as string.
                    None => ReaperCommand::Id(item.command_id.get()),
                    // ReaScripts and custom actions have a command name as persistent identifier.
                    Some(name) => ReaperCommand::Name(name.into_string()),
                };
                Some(cmd)
            },
            invocation: Some(ActionInvocationKind::Absolute7Bit),
            ..Default::default()
        };
        Mapping {
            name: Some(item.label),
            source: Some(Source::StreamDeck(source)),
            glue: Some(glue),
            target: Some(Target::ReaperAction(target)),
            ..Default::default()
        }
    });
    let set_brightness_to_max_mapping = Mapping {
        name: Some("Set brightness to max".to_string()),
        source: Some(Source::RealearnInstanceStart),
        target: Some(Target::StreamDeckBrightness(Default::default())),
        ..Default::default()
    };
    let all_mappings = [set_brightness_to_max_mapping]
        .into_iter()
        .chain(button_mappings);
    let compartment = Compartment {
        mappings: Some(all_mappings.collect()),
        ..Default::default()
    };
    Ok(compartment)
}

fn get_toolbar_cmd_item(
    toolbar_name: &str,
    pos: u32,
) -> Option<Option<CommandItem<String, String>>> {
    Reaper::get()
        .medium_reaper()
        .get_custom_menu_or_toolbar_item(toolbar_name, pos, |result| match result? {
            MenuOrToolbarItem::Command(item) => {
                let owned_command_item = CommandItem {
                    command_id: item.command_id,
                    toolbar_flags: item.toolbar_flags,
                    label: item.label.to_string(),
                    icon_file_name: item.icon_file_name.map(|n| n.to_string()),
                };
                Some(Some(owned_command_item))
            }
            _ => Some(None),
        })
}
