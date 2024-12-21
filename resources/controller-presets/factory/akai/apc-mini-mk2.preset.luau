--- name: APC mini mk2
--- realearn_version: 2.16.0-pre.8
--- author: helgoboss
--- device_manufacturer: Akai
--- device_name: APC mini mk2
--- description: |
---     This controller preset exposes all control elements of the APC mini mk2 in a neutral way. It supports
---     ReaLearn color feedback as well as Playtime slot status feedback.
--- midi_identity_pattern: F0 7E ? 06 02 47 4F 00 19 * F7
--- # This device exposes 2 USB MIDI ports, one for "Notes" and one for "Control". We are
--- # interested in "Control". Both MIDI ports reply with the same device identity. If we
--- # don't provide a MIDI output device pattern, it's possible that ReaLearn's automatic
--- # controller detection feature takes the "Notes" port ... and things wouldn't work!
--- # On Windows, the naming is different, that's why there's also "MIDIOUT2".
--- midi_output_port_patterns: ["macos:*Control*", "windows:APC mini mk2"]
--- provided_schemes: [akai/apc-mini-mk2, grid]

--!strict

-- Configuration

local resolve_shift = false

-- Requires

local preset_runtime = require("preset_runtime")
local realearn = require("realearn")

-- Define MIDI scripts

local common_lua = preset_runtime.include_str("akai/apc-mk2-lib/compartment-common.luau")

local function build_pad_script(pad_index: number): string
    return `return require("compartment").pad_script({pad_index}, y, context)`
end

-- Parameters

local shift_param = realearn.Parameter {
    index = 0,
    name = "Shift",
}
local parameters: { realearn.Parameter }? = if resolve_shift then { shift_param } else nil

-- Single buttons

local function simple_button(id: string, name: string, key_number: number): realearn.Mapping
    return realearn.Mapping {
        id = id,
        name = name,
        feedback_enabled = false,
        source = realearn.Source.MidiNoteVelocity {
            channel = 0,
            key_number = key_number,
        },
        target = realearn.Target.Virtual {
            id = id,
            character = "Button",
        },
    }
end

local mappings: { realearn.Mapping } = {
    -- Main fader
    {
        id = "fm",
        feedback_enabled = false,
        source = realearn.Source.MidiControlChangeValue {
            channel = 0,
            controller_number = 56,
            character = "Range",
        },
        target = realearn.Target.Virtual {
            id = "main/fader",
        },
    },
}

-- Shift
local no_shift_activation_condition: realearn.ActivationCondition?
if resolve_shift then
    -- The activation condition reflecting the state that shift is not pressed.
    no_shift_activation_condition = realearn.ActivationCondition.Modifier {
        modifiers = {
            {
                parameter = 0,
                on = false,
            },
        },
    }
    -- Mapping to make shift button switch to other set of virtual control elements
    local shift_mapping = realearn.Mapping {
        name = "Shift",
        feedback_enabled = false,
        source = realearn.Source.MidiNoteVelocity {
            channel = 0,
            key_number = 122,
        },
        target = realearn.Target.CompartmentParameterValue {
            parameter = realearn.CompartmentParameterDescriptor.ById {
                index = 0,
            },
        },
    }
    table.insert(mappings, shift_mapping)
    -- Alternative set of virtual control elements
    local alt_elements = {
        { key = 104, id = "cursor-up" },
        { key = 105, id = "cursor-down" },
        { key = 106, id = "cursor-left" },
        { key = 107, id = "cursor-right" },
        { key = 100, id = "volume" },
        { key = 101, id = "pan" },
        { key = 102, id = "sends" },
        { key = 103, id = "device" },
        { key = 112, id = "stop-clip" },
        { key = 113, id = "solo" },
        { key = 114, id = "mute" },
        { key = 115, id = "record-arm" },
        { key = 116, id = "track-select" },
        { key = 117, id = "drum" },
        { key = 118, id = "note" },
        { key = 119, id = "stop-all-clips" },
    }
    for _, element in ipairs(alt_elements) do
        local mapping = realearn.Mapping {
            activation_condition = realearn.ActivationCondition.Modifier {
                modifiers = {
                    {
                        parameter = 0,
                        on = true,
                    },
                },
            },
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = element.key,
            },
            target = realearn.Target.Virtual {
                id = element.id,
                character = "Button",
            },
        }
        table.insert(mappings, mapping)
    end
else
    no_shift_activation_condition = nil
    local mapping = simple_button("shift", "Shift", 122)
    table.insert(mappings, mapping)
end

-- Channel faders
for i = 0, 7 do
    local human_i = i + 1
    local mapping = realearn.Mapping {
        id = `f{human_i}`,
        feedback_enabled = false,
        source = realearn.Source.MidiControlChangeValue {
            channel = 0,
            controller_number = 48 + i,
            character = "Range",
        },
        target = realearn.Target.Virtual {
            id = i,
        },
    }
    table.insert(mappings, mapping)
end

-- Clip launch buttons
for col = 0, 7 do
    local human_col = col + 1
    for row = 0, 7 do
        local human_row = row + 1
        local key_number_offset = (7 - row) * 8
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

-- Clip stop buttons
for col = 0, 7 do
    local human_col = col + 1
    local id = `col{human_col}/stop`
    local mapping = realearn.Mapping {
        id = id,
        activation_condition = no_shift_activation_condition,
        source = realearn.Source.MidiNoteVelocity {
            channel = 0,
            key_number = 100 + col,
        },
        target = realearn.Target.Virtual {
            id = id,
            character = "Button",
        },
    }
    table.insert(mappings, mapping)
end

-- Scene launch buttons
for row = 0, 7 do
    local human_row = row + 1
    local id = `row{human_row}/play`
    local mapping = realearn.Mapping {
        id = id,
        activation_condition = no_shift_activation_condition,
        source = realearn.Source.MidiNoteVelocity {
            channel = 0,
            key_number = 112 + row,
        },
        target = realearn.Target.Virtual {
            id = id,
            character = "Button",
        },
    }
    table.insert(mappings, mapping)
end

return realearn.Compartment {
    parameters = parameters,
    mappings = mappings,
    common_lua = common_lua,
    custom_data = {
        grid = {
            column_count = 8,
            row_count = 8,
        },
    },
}
