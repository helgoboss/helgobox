-- Single buttons
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
        feedback_enabled = false,
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
    {
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
    },
}

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
        mappings = mappings,
    },
}