--- name: Launchpad Pro mk2 - Live mode
--- realearn_version: 2.16.0-pre.8
--- author: helgoboss
--- device_manufacturer: Novation
--- device_name: Launchpad Pro mk2
--- description: |
---     This controller preset exposes all control elements of the Launchpad Pro mk2 in a neutral way. It supports
---     ReaLearn color feedback as well as Playtime slot status feedback.
--- # I leave part of the family code dynamic. It's not always 00: https://github.com/helgoboss/helgobox/issues/939
--- midi_identity_pattern: F0 7E ?  06 02 00 20 29 51 00 ? 00 * F7
--- midi_output_port_patterns: ["macos:*Live*", "windows:*Launchpad Pro"]
--- provided_schemes: [novation/launchpad-pro-mk2/live, grid]

--!strict

-- Requires

local preset_runtime = require("preset_runtime")
local realearn = require("realearn")

-- Define MIDI scripts

local common_lua = preset_runtime.include_str("novation/launchpad-lib/compartment-common.luau")

local function build_pad_script(pad_index: number): string
    return `return require("compartment").pad_script({pad_index}, y, context)`
end

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
    name = "Enter live mode",
    source = realearn.Source.MidiScript {
        script_kind = "Lua",
        script = [[
local live_mode = 0x00
local standalone_mode = 0x01
local mode = if y == nil then standalone_mode else live_mode

return {
    messages = {
        { 0xF0, 0x00, 0x20, 0x29, 0x02, 0x10, 0x21, mode, 0xF7 }
    }
}
        ]]
    },
    target = realearn.Target.Dummy {
    }
}

local mappings = {
    init_mapping,
    simple_button("cursor-up", "Up", 91),
    simple_button("cursor-down", "Down", 92),
    simple_button("cursor-left", "Left", 93),
    simple_button("cursor-right", "Right", 94),
    simple_button("session", "Session", 95),
    simple_button("note", "Note", 96),
    simple_button("device", "Device", 97),
    simple_button("user", "User", 98),
    simple_button("shift", "Shift", 80),
    simple_button("click", "Click", 70),
    simple_button("undo", "Undo", 60),
    simple_button("delete", "Delete", 50),
    simple_button("quantize", "Quantise", 40),
    simple_button("duplicate", "Duplicate", 30),
    simple_button("double", "Double", 20),
    simple_button("record", "Record", 10),
    simple_button("record-arm", "Record Arm", 1),
    simple_button("track-select", "Track Select", 2),
    simple_button("mute", "Mute", 3),
    simple_button("solo", "Solo", 4),
    simple_button("volume", "Volume", 5),
    simple_button("pan", "Pan", 6),
    simple_button("sends", "Sends", 7),
    simple_button("stop-clip", "Stop Clip", 8),
}

-- Clip launch buttons

for col = 0, 7 do
    local human_col = col + 1
    for row = 0, 7 do
        local human_row = row + 1
        local key_number_offset = 11 + (7 - row) * 10
        local id = `col{human_col}/row{human_row}/pad`
        local control_mapping = realearn.Mapping {
            id = id,
            feedback_enabled = false,
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = key_number_offset + col,
            },
            target = realearn.Target.Virtual {
                id = id,
                character = "Multi",
            },
        }
        local feedback_mapping = realearn.Mapping {
            id = `{id}-feedback`,
            control_enabled = false,
            source = realearn.Source.MidiScript {
                script_kind = "Lua",
                script = build_pad_script(key_number_offset + col),
            },
            target = realearn.Target.Virtual {
                id = id,
                character = "Multi",
            },
        }
        table.insert(mappings, control_mapping)
        table.insert(mappings, feedback_mapping)
    end
end

-- Scene launch buttons
for row = 0, 7 do
    local human_row = row + 1
    local id = `row{human_row}/play`
    local mapping = realearn.Mapping {
        id = id,
        source = realearn.Source.MidiControlChangeValue {
            channel = 0,
            controller_number = 19 + (7 - row) * 10,
            character = "Button",
        },
        target = realearn.Target.Virtual {
            id = id,
            character = "Button",
        },
    }
    table.insert(mappings, mapping)
end

return realearn.Compartment {
    common_lua = common_lua,
    mappings = mappings,
    custom_data = {
        grid = {
            column_count = 8,
            row_count = 8,
        },
    }
}