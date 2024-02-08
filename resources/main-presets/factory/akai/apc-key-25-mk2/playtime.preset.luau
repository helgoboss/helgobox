--- name: APC Key 25 mk2 - Playtime
--- author: helgoboss
--- realearn_version: 2.16.0-pre.8
--- used_schemes: [akai/apc-key-25-mk2]
--- required_features: [playtime]

--!strict


local commons = require("akai/apc-key-25-lib/playtime-commons")

return commons.create_compartment {
    use_column_stop_buttons = true,
    mute_track_button_id = "row3/play",
    arm_track_button_id = "row4/play",
}