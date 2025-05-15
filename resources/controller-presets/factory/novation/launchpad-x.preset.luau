--- name: Launchpad X - Live mode
--- realearn_version: 2.16.0-pre.8
--- author: helgoboss
--- device_manufacturer: Novation
--- device_name: Launchpad X
--- description: |
---     This controller preset exposes all control elements of the Launchpad X in a neutral way. It supports
---     ReaLearn color feedback as well as Playtime slot status feedback.
--- midi_identity_pattern: F0 7E ?  06 02 00 20 29 03 01 00 00 * F7
--- midi_output_port_patterns: ["macos:*DAW*", "windows:LPX MIDI"]
--- provided_schemes: [novation/launchpad-x/live, grid]

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
    name = "Enter DAW mode and select session layout",
    source = realearn.Source.MidiScript {
        script_kind = "Lua",
        script = [[
local standalone_mode = 0x00
local daw_mode = 0x01
local session_layout = 0x00
local mode = if y == nil then standalone_mode else daw_mode
local enter_or_leave_daw_mode_msg = { 0xF0, 0x00, 0x20, 0x29, 0x02, 0x0C, 0x10, mode, 0xF7 }
local select_session_layout = { 0xF0, 0x00, 0x20, 0x29, 0x02, 0x0C, 0x00, session_layout, 0xF7 }

local messages = {
    enter_or_leave_daw_mode_msg,
}
if y then
    table.insert(messages, select_session_layout)
end

return {
    messages = messages
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
    simple_button("custom", "Custom", 97),
    simple_button("record", "Record", 98),
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