--- name: Launchpad mk1 - Live mode
--- realearn_version: 2.16.0-pre.8
--- author: helgoboss
--- description: |
---     This controller preset exposes all control elements of the Launchpad mk1 in a neutral way. It supports Playtime slot status feedback.
---
---     This device cannot be auto-detected because it doesn't support the MIDI device inquiry protocol.
--- device_manufacturer: Novation
--- device_name: Launchpad mk1
--- provided_schemes: [novation/launchpad-mk1, grid]

--!strict

-- Requires

local realearn = require("realearn")

-- Single buttons

local function simple_button(id: string, name: string, cc: number): realearn.Mapping
    return realearn.Mapping {
        id = id,
        name = name,
        source = realearn.Source.MidiControlChangeValue {
            channel = 0,
            controller_number = cc,
            character = "Button",
        },
        target = realearn.Target.Virtual {
            id = id,
            character = "Button",
        },
    }
end

local init_mapping = realearn.Mapping {
    name = "Enable flashing LEDs",
    source = realearn.Source.MidiRaw {
        pattern = "B0 00 28",
    },
    target = realearn.Target.Dummy {
    }
}

local mappings = {
    init_mapping,
    simple_button("cursor-up", "Up", 104),
    simple_button("cursor-down", "Down", 105),
    simple_button("cursor-left", "Left", 106),
    simple_button("cursor-right", "Right", 107),
    simple_button("session", "session", 108),
    simple_button("user-1", "user 1", 109),
    simple_button("user-2", "user 2", 110),
    simple_button("mixer", "mixer", 111),
}

-- Clip launch buttons

local leds = {
    off = 12,
    red_low = 13,
    red_full = 15,
    amber_low = 29,
    amber_full = 63,
    yellow_full = 62,
    green_low = 28,
    green_full = 60, 
    red_flashing = 11,
    amber_flashing = 59,
    yellow_flashing = 58,
    green_flashing = 56,
}

for col = 0, 7 do
    local human_col = col + 1
    for row = 0, 7 do
        local human_row = row + 1
        local key_number_offset = row * 16
        local id = `col{human_col}/row{human_row}/pad`
        local control_mapping = realearn.Mapping {
            id = id,
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = key_number_offset + col,
            },
            glue = realearn.Glue {
                feedback_value_table = {
                    kind = "FromTextToDiscrete",
                    value = {
                        ["playtime.slot_state.empty"] = leds.off,
                        ["playtime.slot_state.armed"] = leds.red_low,
                        ["playtime.slot_state.stopped"] = leds.amber_low,
                        ["playtime.slot_state.ignited"] = leds.amber_low,
                        ["playtime.slot_state.scheduled_for_play_start"] = leds.green_flashing,
                        ["playtime.slot_state.playing"] = leds.green_full,
                        ["playtime.slot_state.paused"] = leds.amber_low,
                        ["playtime.slot_state.scheduled_for_play_stop"] = leds.amber_flashing,
                        ["playtime.slot_state.scheduled_for_play_restart"] = leds.green_flashing,
                        ["playtime.slot_state.scheduled_for_record_start"] = leds.red_flashing,
                        ["playtime.slot_state.recording"] = leds.red_full,
                        ["playtime.slot_state.scheduled_for_record_stop"] = leds.green_flashing,
                    },
                }
            },
            target = realearn.Target.Virtual {
                id = id,
                character = "Multi",
            },
        }
        table.insert(mappings, control_mapping)
    end
end

-- Scene launch buttons
for row = 0, 7 do
    local human_row = row + 1
    local id = `row{human_row}/play`
    local mapping = realearn.Mapping {
        id = id,
        source = realearn.Source.MidiNoteVelocity {
            channel = 0,
            key_number = 8 + row * 16,
        },
        target = realearn.Target.Virtual {
            id = id,
            character = "Button",
        },
    }
    table.insert(mappings, mapping)
end

return realearn.Compartment {
    mappings = mappings,
    custom_data = {
        grid = {
            column_count = 8,
            row_count = 8,
        },
    }
}