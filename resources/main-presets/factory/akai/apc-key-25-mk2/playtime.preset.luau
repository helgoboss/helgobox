--- name: APC Key 25 mk2 - Playtime
--- realearn_version: 2.16.0-pre.8
--- author: helgoboss
--- description: |
---     This main preset turns the APC Key 25 mk2 into a capable device for controlling Playtime. Hold Shift
---     to access advanced functions.
--- used_schemes: [akai/apc-key-25-mk2]
--- required_features: [playtime]

--!strict


local commons = require("akai/apc-lib/playtime-commons")

return commons.create_compartment {
    use_column_stop_buttons = true,
    stop_column_if_slot_empty = true,
    mute_track_button_id = "row3/play",
    arm_track_button_id = "row4/play",
    column_count = 8,
    row_count = 5,
    up_button_id = "col1/stop",
    down_button_id = "col2/stop",
    left_button_id = "col3/stop",
    right_button_id = "col4/stop",
    volume_button_id = "col5/stop",
    pan_button_id = "col6/stop",
    send_button_id = "col7/stop",
    device_button_id = "col8/stop",
    stop_all_clips_button_id = "stop-all-clips",
    stop_all_clips_need_shift = false,
}