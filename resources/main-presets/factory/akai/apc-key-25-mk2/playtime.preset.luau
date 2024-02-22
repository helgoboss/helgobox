--- name: APC Key 25 mk2 - Playtime
--- realearn_version: 2.16.0-pre.8
--- author: helgoboss
--- description: |
---     This main preset turns the APC Key 25 mk2 into a capable device for controlling Playtime. Hold Shift
---     to access advanced functions.
--- used_schemes: [akai/apc-key-25-mk2]
--- required_features: [playtime]

--!strict


local commons = require("akai/apc-key-25-lib/playtime-commons")

return commons.create_compartment {
    use_column_stop_buttons = true,
    stop_column_if_slot_empty = true,
    mute_track_button_id = "row3/play",
    arm_track_button_id = "row4/play",
}