-- Configuration
local resolve_shift = true

-- Single buttons
local parameters = {}
local mappings = {
    {
        id = "ac49cd8a-cd98-4acd-84a0-276372aa8d05",
        name = "SAC",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 81,
        },
        target = {
            kind = "Virtual",
            id = "stop-all-clips",
            character = "Button",
        },
    },
    {
        id = "22050182-480e-4267-b203-9ad641a72b44",
        name = "Sustain",
        feedback_enabled = false,
        source = {
            kind = "MidiControlChangeValue",
            channel = 1,
            controller_number = 64,
            character = "Button",
        },
        target = {
            kind = "Virtual",
            id = "sustain",
            character = "Button",
        },
    },
    {
        id = "b09d6169-a7df-4dbe-b76c-73d30c8625c3",
        name = "Play",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 91,
        },
        target = {
            kind = "Virtual",
            id = "play",
            character = "Button",
        },
    },
    {
        id = "6c4745ac-39bb-4ed7-b290-2cc5aef49bbb",
        name = "Rec",
        feedback_enabled = false,
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 93,
        },
        target = {
            kind = "Virtual",
            id = "record",
            character = "Button",
        },
    },
}

-- Shift
local no_shift_activation_condition
if resolve_shift then
    -- The activation condition reflecting the state that shift is not pressed.
    no_shift_activation_condition = {
        kind = "Modifier",
        modifiers = {
            {
                parameter = 0,
                on = false,
            },
        },
    }
    -- The shift modifier parameter
    local parameter = {
        index = 0,
        name = "Shift",
    }
    table.insert(parameters, parameter)
    -- Mapping to make shift button switch to other set of virtual control elements
    local shift_mapping = {
        name = "Shift",
        feedback_enabled = false,
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 98,
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 100,
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
    }
    for _, element in ipairs(alt_elements) do
        local mapping = {
            activation_condition = {
                kind = "Modifier",
                modifiers = {
                    {
                        parameter = 0,
                        on = true,
                    },
                },
            },
            source = {
                kind = "MidiNoteVelocity",
                channel = 0,
                key_number = element.key,
            },
            target = {
                kind = "Virtual",
                id = element.id,
                character = "Button",
            },
        }
        table.insert(mappings, mapping)
    end
else
    no_shift_activation_condition = nil
    local mapping = {
        id = "838cc9e6-5857-4dd2-952b-339f3f886f3d",
        name = "Shift",
        feedback_enabled = false,
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 98,
        },
        target = {
            kind = "Virtual",
            id = "shift",
            character = "Button",
        },
    }
    table.insert(mappings, mapping)
end

-- Knobs
for i = 0, 7 do
    local human_i = i + 1
    local mapping = {
        id = "k" .. human_i,
        feedback_enabled = false,
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 48 + i,
        },
        target = {
            kind = "Virtual",
            id = i,
        },
    }
    table.insert(mappings, mapping)
end

-- Clip launch buttons
local feedback_value_table = {
    -- Off
    empty = 0,
    -- Yellow
    stopped = 5,
    -- Green blinking
    scheduled_for_play_start = 2,
    -- Green
    playing = 1,
    -- Yellow
    paused = 5,
    -- Yellow blinking
    scheduled_for_play_stop = 6,
    -- Red blinking
    scheduled_for_record_start = 4,
    -- Red
    recording = 3,
    -- Yellow blinking
    -- TODO-high Might be better to distinguish between scheduled_for_stop or scheduled_for_play_start instead.
    scheduled_for_record_stop = 6,
}

for col = 0, 7 do
    local human_col = col + 1
    for row = 0, 4 do
        local human_row = row + 1
        local key_number_offset = (4 - row) * 8
        local id = "col" .. human_col .. "/row" .. human_row .. "/pad"
        local mapping = {
            id = id,
            source = {
                kind = "MidiNoteVelocity",
                channel = 0,
                key_number = key_number_offset + col,
            },
            glue = {
                feedback_value_table = feedback_value_table,
            },
            target = {
                kind = "Virtual",
                id = id,
                character = "Button",
            },
        }
        table.insert(mappings, mapping)
    end
end

-- Clip stop buttons
for col = 0, 7 do
    local human_col = col + 1
    local id = "col" .. human_col .. "/stop"
    local mapping = {
        id = id,
        activation_condition = no_shift_activation_condition,
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 64 + col,
        },
        target = {
            kind = "Virtual",
            id = id,
            character = "Button",
        },
    }
    table.insert(mappings, mapping)
end

-- Scene launch buttons
for row = 0, 4 do
    local human_row = row + 1
    local id = "row" .. human_row .. "/play"
    local mapping = {
        id = id,
        activation_condition = no_shift_activation_condition,
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 82 + row,
        },
        target = {
            kind = "Virtual",
            id = id,
            character = "Button",
        },
    }
    table.insert(mappings, mapping)
end

return {
    kind = "ControllerCompartment",
    value = {
        parameters = parameters,
        mappings = mappings,
    },
}