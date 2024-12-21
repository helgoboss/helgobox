--- name: APC mini
--- realearn_version: 2.16.0-pre.8
--- author: helgoboss
--- device_manufacturer: Akai
--- device_name: APC mini
--- description: |
---     This controller preset exposes all control elements of the APC mini mk2 in a neutral way. It supports
---     Playtime slot status feedback.
--- midi_identity_pattern: F0 7E ? 06 02 47 28 00 19 * F7
--- provided_schemes: [akai/apc-mini, grid]

--!strict

-- Configuration

local resolve_shift = false

-- Requires

local realearn = require("realearn")

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
            key_number = 98,
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
        { key = 64, id = "cursor-up" },
        { key = 65, id = "cursor-down" },
        { key = 66, id = "cursor-left" },
        { key = 67, id = "cursor-right" },
        { key = 68, id = "volume" },
        { key = 69, id = "pan" },
        { key = 70, id = "sends" },
        { key = 71, id = "device" },
        { key = 82, id = "stop-clip" },
        { key = 83, id = "solo" },
        { key = 84, id = "record-arm" },
        { key = 85, id = "mute" },
        { key = 86, id = "track-select" },
        { key = 89, id = "stop-all-clips" },
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
    local mapping = simple_button("shift", "Shift", 98)
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
local feedback_value_table = realearn.FeedbackValueTable.FromTextToDiscrete {
    value = {
        -- Off
        ["playtime.slot_state.empty"] = 0,
        -- Red
        ["playtime.slot_state.armed"] = 0,
        -- Yellow
        ["playtime.slot_state.stopped"] = 5,
        -- Green blinking
        ["playtime.slot_state.scheduled_for_play_start"] = 2,
        -- Green
        ["playtime.slot_state.playing"] = 1,
        -- Yellow
        ["playtime.slot_state.paused"] = 5,
        -- Yellow blinking
        ["playtime.slot_state.scheduled_for_play_stop"] = 6,
        -- Yellow blinking
        ["playtime.slot_state.scheduled_for_play_restart"] = 6,
        -- Red blinking
        ["playtime.slot_state.scheduled_for_record_start"] = 4,
        -- Red
        ["playtime.slot_state.recording"] = 3,
        -- Yellow blinking
        ["playtime.slot_state.scheduled_for_record_stop"] = 6,
    },
}
for col = 0, 7 do
    local human_col = col + 1
    for row = 0, 7 do
        local human_row = row + 1
        local key_number_offset = (7 - row) * 8
        local id = `col{human_col}/row{human_row}/pad`
        local mapping = realearn.Mapping {
            id = id,
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = key_number_offset + col,
            },
            glue = {
                feedback_value_table = feedback_value_table,
            },
            target = realearn.Target.Virtual {
                id = id,
                character = "Multi",
            },
        }
        table.insert(mappings, mapping)
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
            key_number = 64 + col,
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
            key_number = 82 + row,
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
    custom_data = {
        grid = {
            column_count = 8,
            row_count = 8,
        },
    },
}
