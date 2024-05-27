--- name: Launchpad Mini mk3 - Playtime
--- realearn_version: 2.16.0-pre.8
--- author: helgoboss
--- description: |
---     This main preset turns the Launchpad Mini mk3 into a capable device for controlling Playtime.
--- used_schemes: [novation/launchpad-mini-mk3/live]
--- required_features: [playtime]

--!strict

local commons = require("novation/launchpad-lib/playtime-commons")

return commons.create_compartment {
    pad_character = "Button",
    stop_column_if_slot_empty = true,
    has_dedicated_column_action_buttons = false,
    has_shift_button = false,
    has_delete_button = false,
    has_quantize_button = false,
    has_fixed_length_button = false,
    has_duplicate_button = false,
    has_play_button = false,
    has_record_button = false,
    undo_button_id = nil,
    undo_needs_shift = true,
    redo_button_id = nil,
    redo_needs_shift = true,
    click_button_id = nil,
    click_needs_shift = true,
    double_button_id = nil,
    double_needs_shift = true,
    tap_button_id = nil,
    tap_needs_shift = true,
    use_last_row_play_button_for_column_modes = true,
}