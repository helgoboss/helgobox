local channel_count = 8;

local groups = {
    {
        key = "fader",
        name = "Fader",
    },
    {
        key = "v-pot",
        name = "V-Pot",
    },
    {
        key = "v-select",
        name = "V-Select",
    },
    {
        key = "fader-touch",
        name = "Fader Touch",
    },
    {
        key = "select",
        name = "Select",
    },
    {
        key = "mute",
        name = "Mute",
    },
    {
        key = "solo",
        name = "Solo",
    },
    {
        key = "record-ready",
        name = "Record-ready",
    },
    {
        key = "v-pot-leds",
        name = "V-Pot LEDs",
    },
    {
        key = "lcd",
        name = "LCD",
    },
}

local mappings = {
    {
        group = "fader",
        source = {
            kind = "MidiPitchBendChangeValue",
            channel = 8,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "main/fader",
        },
    },
    {
        feedback_enabled = false,
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 60,
            character = "Relative1",
            fourteen_bit = false,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "jog",
        },
    },
    {
        group = "fader-touch",
        feedback_enabled = false,
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 112,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "main/fader/touch",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 84,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "marker",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 74,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "read",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 75,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "write",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 48,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "ch-left",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 49,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "ch-right",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 46,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "bank-left",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 47,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "bank-right",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 91,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "rewind",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 92,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "fast-fwd",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 94,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "play",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 93,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "stop",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 95,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "record",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 86,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "cycle",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 100,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "zoom",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 101,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "scrub",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 98,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "cursor-left",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 99,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "cursor-right",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 96,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "cursor-up",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 97,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "cursor-down",
            character = "Button",
        },
    },
    
    


    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 85,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "nudge",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 87,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "drop",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 88,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "replace",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 89,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "click",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 90,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "solo",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 54,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "f1",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 55,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "f2",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 56,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "f3",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 57,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "f4",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 58,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "f5",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 59,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "f6",
            character = "Button",
        },
    },
    {
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 53,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "smpte-beats",
            character = "Button",
        },
    },
    {
        group = "lcd",
        source = {
            kind = "MackieSevenSegmentDisplay",
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = "lcd/assignment",
        },
    },
}

-- For each channel
for ch = 0, channel_count - 1 do
    local prefix = "ch"..(ch+1).."/"
    local v_select = {
        group = "v-select",
        feedback_enabled = false,
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 32 + ch,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = prefix.."v-select",
            character = "Button",
        },
    }
    local fader_touch = {
        group = "fader-touch",
        feedback_enabled = false,
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 104 + ch,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = prefix.."fader/touch",
            character = "Button",
        },
    }
    local select = {
        group = "select",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 24 + ch,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = prefix.."select",
            character = "Button",
        },
    }
    local fader = {
        group = "fader",
        source = {
            kind = "MidiPitchBendChangeValue",
            channel = ch,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = prefix.."fader",
        },
    }
    local v_pot_control = {
        group = "v-pot",
        feedback_enabled = false,
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 16 + ch,
            character = "Relative1",
            fourteen_bit = false,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = prefix.."v-pot",
        },
    }
    local v_pot_feedback = {
        group = "v-pot-leds",
        control_enabled = false,
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 48 + ch,
            fourteen_bit = false,
        },
        glue = {
            source_interval = {0.25984251968503935, 0.33858267716535434},
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = prefix.."v-pot",
        },
    }
    local mute = {
        group = "mute",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 16 + ch,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = prefix.."mute",
            character = "Button",
        },
    }
    local solo = {
        group = "solo",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 8 + ch,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = prefix.."solo",
            character = "Button",
        },
    }
    local record_ready = {
        group = "record-ready",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 0 + ch,
        },
        glue = {
            step_size_interval = {0.01, 0.01},
            step_factor_interval = {1, 1},
        },
        target = {
            kind = "Virtual",
            id = prefix.."record-ready",
            character = "Button",
        },
    }
    table.insert(mappings, v_select)
    table.insert(mappings, fader_touch)
    table.insert(mappings, select)
    table.insert(mappings, fader)
    table.insert(mappings, v_pot_control)
    table.insert(mappings, v_pot_feedback)
    table.insert(mappings, mute)
    table.insert(mappings, solo)
    table.insert(mappings, record_ready)
end

return {
    kind = "ControllerCompartment",
    value = {
        groups = groups,
        mappings = mappings
    },
}