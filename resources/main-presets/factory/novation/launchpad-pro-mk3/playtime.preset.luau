--- name: Launchpad Pro mk3 - Playtime
--- realearn_version: 2.16.0-pre.8
--- author: helgoboss
--- description: |
---     This main preset turns the Launchpad Pro mk3 into a capable device for controlling Playtime.
---     
---     The following features are not yet supported: Note, Chord, Custom, Sequencer, Projects, Volume, Pan, Sends, Device, Record,
---     Record Quantise, Swing, Tempo, Tap, Capture MIDI, Fixed Length, Play, Patterns, Steps, Pattern Settings, Velocity, Probability,
---     Mutation, Micro Step, Print to Clip.
--- used_schemes: [novation/launchpad-pro-mk3/live]
--- required_features: [playtime]

--!strict

local commons = require("novation/launchpad-lib/playtime-commons")

return commons.create_compartment {
    stop_column_if_slot_empty = true,
    has_dedicated_column_action_buttons = true,
    
    has_shift_button = true,
    has_delete_button = true,
    has_quantize_button = true,
    has_fixed_length_button = true,
    has_duplicate_button = true,
    has_play_button = true,
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

    undo_button_id = "record-arm",
    undo_needs_shift = true,
    redo_button_id = "mute",
    redo_needs_shift = true,
    click_button_id = "solo",
    click_needs_shift = true,
    double_button_id = "duplicate",
    double_needs_shift = true,
    tap_button_id = "sends",
    tap_needs_shift = true,
    use_last_row_play_button_for_column_modes = false,
    mixer_button_id = nil,
}