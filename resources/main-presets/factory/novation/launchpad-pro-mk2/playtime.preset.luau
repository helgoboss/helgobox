--- name: Launchpad Pro mk2 - Playtime
--- realearn_version: 2.16.0-pre.8
--- author: helgoboss
--- device_manufacturer: Novation
--- device_name: Launchpad Pro mk2
--- description: |
---     This main preset turns the Launchpad Pro mk2 into a capable device for controlling Playtime.
---     
---     The following features are not yet supported: Note, Device, User, Volume, Pan, Sends, Record,
---     Record Quantise.
--- used_schemes: [novation/launchpad-pro-mk2/live]
--- required_features: [playtime]

--!strict

local commons = require("novation/launchpad-lib/playtime-commons")

return commons.create_compartment {
    stop_column_if_slot_empty = true,
    has_dedicated_column_action_buttons = false,
    has_shift_button = true,
    has_delete_button = true,
    has_quantize_button = true,
    has_fixed_length_button = false,
    has_duplicate_button = true,
    has_play_button = false,
    has_record_button = true,

    stop_clip_button_id = "stop-clip",
    stop_clip_needs_mixer = false,
    mute_button_id = "mute",
    mute_needs_mixer = false,
    solo_button_id = "solo",
    solo_needs_mixer = false,
    record_arm_button_id = "record-arm",
    record_arm_needs_mixer = false,
    track_select_button_id = "track-select",

    undo_button_id = "undo",
    undo_needs_shift = false,
    redo_button_id = "undo",
    redo_needs_shift = true,
    click_button_id = "click",
    click_needs_shift = false,
    double_button_id = "double",
    double_needs_shift = false,
    tap_button_id = nil,
    tap_needs_shift = false,
    use_last_row_play_button_for_column_modes = false,
    mixer_button_id = nil,
}