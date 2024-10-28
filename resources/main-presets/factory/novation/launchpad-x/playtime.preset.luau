--- name: Launchpad X - Playtime
--- realearn_version: 2.16.0-pre.8
--- author: helgoboss
--- device_manufacturer: Novation
--- device_name: Launchpad X
--- description: |
---     This main preset turns the Launchpad X into a capable device for controlling Playtime.
---     
---     The following features are not yet supported: Note, Custom, Volume, Pan, Send A, Send B, Capture MIDI
--- used_schemes: [novation/launchpad-x/live]
--- required_features: [playtime]

--!strict

local commons = require("novation/launchpad-lib/playtime-commons")

return commons.create_compartment {
    stop_column_if_slot_empty = true,
    has_dedicated_column_action_buttons = false,
    has_shift_button = false,
    has_delete_button = false,
    has_quantize_button = false,
    has_fixed_length_button = false,
    has_duplicate_button = false,
    has_play_button = false,
    has_record_button = true,

    stop_clip_button_id = "row5/play",
    stop_clip_needs_mixer = true,
    mute_button_id = "row6/play",
    mute_needs_mixer = true,
    solo_button_id = "row7/play",
    solo_needs_mixer = true,
    record_arm_button_id = "row8/play",
    record_arm_needs_mixer = true,
    track_select_button_id = nil,

    undo_button_id = nil,
    undo_needs_shift = false,
    redo_button_id = nil,
    redo_needs_shift = false,
    click_button_id = nil,
    click_needs_shift = false,
    double_button_id = nil,
    double_needs_shift = false,
    tap_button_id = nil,
    tap_needs_shift = false,
    use_last_row_play_button_for_column_modes = false,
    mixer_button_id = "session",
    use_mk1_colors = false,
}