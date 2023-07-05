local channel_count = 8;

-- Replace with "XTouchMackieLcd" in order to be able to benefit from colors when using X-Touch.
local lcd_kind = "MackieLcd";

local binary_eight = {
    [0] = "000",
    [1] = "001",
    [2] = "010",
    [3] = "011",
    [4] = "100",
    [5] = "101",
    [6] = "110",
    [7] = "111",
}

local groups = {
    {
        id = "fader",
        name = "Fader",
    },
    {
        id = "v-pot",
        name = "V-Pot",
    },
    {
        id = "v-select",
        name = "V-Select",
    },
    {
        id = "fader-touch",
        name = "Fader Touch",
    },
    {
        id = "select",
        name = "Select",
    },
    {
        id = "mute",
        name = "Mute",
    },
    {
        id = "solo",
        name = "Solo",
    },
    {
        id = "record-ready",
        name = "Record-ready",
    },
    {
        id = "v-pot-leds",
        name = "V-Pot LEDs",
    },
    {
        id = "lcd",
        name = "LCD",
    },
    {
        id = "meter",
        name = "Meter",
    },
}

local mappings = {
    {
        id = "main/fader",
        group = "fader",
        source = {
            kind = "MidiPitchBendChangeValue",
            channel = 8,
        },
        target = {
            kind = "Virtual",
            id = "main/fader",
        },
    },
    {
        id = "jog",
        feedback_enabled = false,
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 60,
            character = "Relative1",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "jog",
        },
    },
    {
        id = "main/fader/touch",
        group = "fader-touch",
        feedback_enabled = false,
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 112,
        },
        target = {
            kind = "Virtual",
            id = "main/fader/touch",
            character = "Button",
        },
    },
    {
        id = "marker",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 84,
        },
        target = {
            kind = "Virtual",
            id = "marker",
            character = "Button",
        },
    },
    {
        id = "read",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 74,
        },
        target = {
            kind = "Virtual",
            id = "read",
            character = "Button",
        },
    },
    {
        id = "write",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 75,
        },
        target = {
            kind = "Virtual",
            id = "write",
            character = "Button",
        },
    },
    {
        id = "ch-left",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 48,
        },
        target = {
            kind = "Virtual",
            id = "ch-left",
            character = "Button",
        },
    },
    {
        id = "ch-right",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 49,
        },
        target = {
            kind = "Virtual",
            id = "ch-right",
            character = "Button",
        },
    },
    {
        id = "bank-left",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 46,
        },
        target = {
            kind = "Virtual",
            id = "bank-left",
            character = "Button",
        },
    },
    {
        id = "bank-right",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 47,
        },
        target = {
            kind = "Virtual",
            id = "bank-right",
            character = "Button",
        },
    },
    {
        id = "rewind",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 91,
        },
        target = {
            kind = "Virtual",
            id = "rewind",
            character = "Button",
        },
    },
    {
        id = "fast-fwd",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 92,
        },
        target = {
            kind = "Virtual",
            id = "fast-fwd",
            character = "Button",
        },
    },
    {
        id = "play",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 94,
        },
        target = {
            kind = "Virtual",
            id = "play",
            character = "Button",
        },
    },
    {
        id = "stop",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 93,
        },
        target = {
            kind = "Virtual",
            id = "stop",
            character = "Button",
        },
    },
    {
        id = "record",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 95,
        },
        target = {
            kind = "Virtual",
            id = "record",
            character = "Button",
        },
    },
    {
        id = "cycle",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 86,
        },
        target = {
            kind = "Virtual",
            id = "cycle",
            character = "Button",
        },
    },
    {
        id = "zoom",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 100,
        },
        target = {
            kind = "Virtual",
            id = "zoom",
            character = "Button",
        },
    },
    {
        id = "scrub",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 101,
        },
        target = {
            kind = "Virtual",
            id = "scrub",
            character = "Button",
        },
    },
    {
        id = "cursor-left",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 98,
        },
        target = {
            kind = "Virtual",
            id = "cursor-left",
            character = "Button",
        },
    },
    {
        id = "cursor-right",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 99,
        },
        target = {
            kind = "Virtual",
            id = "cursor-right",
            character = "Button",
        },
    },
    {
        id = "cursor-up",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 96,
        },
        target = {
            kind = "Virtual",
            id = "cursor-up",
            character = "Button",
        },
    },
    {
        id = "cursor-down",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 97,
        },
        target = {
            kind = "Virtual",
            id = "cursor-down",
            character = "Button",
        },
    },
    {
        id = "nudge",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 85,
        },
        target = {
            kind = "Virtual",
            id = "nudge",
            character = "Button",
        },
    },
    {
        id = "drop",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 87,
        },
        target = {
            kind = "Virtual",
            id = "drop",
            character = "Button",
        },
    },
    {
        id = "replace",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 88,
        },
        target = {
            kind = "Virtual",
            id = "replace",
            character = "Button",
        },
    },
    {
        id = "click",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 89,
        },
        target = {
            kind = "Virtual",
            id = "click",
            character = "Button",
        },
    },
    {
        id = "solo",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 90,
        },
        target = {
            kind = "Virtual",
            id = "solo",
            character = "Button",
        },
    },
    {
        id = "f1",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 54,
        },
        target = {
            kind = "Virtual",
            id = "f1",
            character = "Button",
        },
    },
    {
        id = "f2",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 55,
        },
        target = {
            kind = "Virtual",
            id = "f2",
            character = "Button",
        },
    },
    {
        id = "f3",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 56,
        },
        target = {
            kind = "Virtual",
            id = "f3",
            character = "Button",
        },
    },
    {
        id = "f4",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 57,
        },
        target = {
            kind = "Virtual",
            id = "f4",
            character = "Button",
        },
    },
    {
        id = "f5",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 58,
        },
        target = {
            kind = "Virtual",
            id = "f5",
            character = "Button",
        },
    },
    {
        id = "f6",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 59,
        },
        target = {
            kind = "Virtual",
            id = "f6",
            character = "Button",
        },
    },
    {
        id = "smpte-beats",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 53,
        },
        target = {
            kind = "Virtual",
            id = "smpte-beats",
            character = "Button",
        },
    },
    {
        id = "lcd/assignment",
        group = "lcd",
        control_enabled = false,
        source = {
            kind = "MackieSevenSegmentDisplay",
        },
        target = {
            kind = "Virtual",
            id = "lcd/assignment",
        },
    },
    {
        id = "lcd/timecode",
        group = "lcd",
        control_enabled = false,
        source = {
            kind = "MackieSevenSegmentDisplay",
            scope = "Tc",
        },
        target = {
            kind = "Virtual",
            id = "lcd/timecode",
        },
    },
}

-- For each channel
for ch = 0, channel_count - 1 do
    local human_ch = ch + 1
    local prefix = "ch"..human_ch.."/"
    local v_select = {
        id = prefix.."v-select",
        group = "v-select",
        feedback_enabled = false,
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 32 + ch,
        },
        target = {
            kind = "Virtual",
            id = prefix.."v-select",
            character = "Button",
        },
    }
    local fader_touch = {
        id = prefix.."fader/touch",
        group = "fader-touch",
        feedback_enabled = false,
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 104 + ch,
        },
        target = {
            kind = "Virtual",
            id = prefix.."fader/touch",
            character = "Button",
        },
    }
    local select = {
        id = prefix.."select",
        group = "select",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 24 + ch,
        },
        target = {
            kind = "Virtual",
            id = prefix.."select",
            character = "Button",
        },
    }
    local fader = {
        id = prefix.."fader",
        group = "fader",
        source = {
            kind = "MidiPitchBendChangeValue",
            channel = ch,
        },
        target = {
            kind = "Virtual",
            id = prefix.."fader",
        },
    }
    local v_pot_control = {
        id = prefix.."v-pot/control",
        group = "v-pot",
        feedback_enabled = false,
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 16 + ch,
            character = "Relative3",
            fourteen_bit = false,
        },
        glue = {
            step_factor_interval = {1, 100},
        },
        target = {
            kind = "Virtual",
            id = prefix.."v-pot",
        },
    }
    local v_pot_feedback_default = {
        id = prefix.."v-pot/feedback",
        group = "v-pot-leds",
        control_enabled = false,
        source = {
            kind = "MidiRaw",
            pattern = "B0 3"..ch.." [0000 dcba]",
        },
        glue = {
            source_interval = {0, 0.75},
        },
        target = {
            kind = "Virtual",
            id = prefix.."v-pot",
        },
    }
    local v_pot_feedback_wrap = {
        id = prefix.."v-pot/wrap",
        group = "v-pot-leds",
        control_enabled = false,
        source = {
            kind = "MidiRaw",
            pattern = "B0 3"..ch.." [0010 dcba]",
        },
        glue = {
            source_interval = {0, 0.75},
        },
        target = {
            kind = "Virtual",
            id = prefix.."v-pot/wrap",
        },
    }
    local v_pot_feedback_boost_cut = {
        id = prefix.."v-pot/boost-cut",
        group = "v-pot-leds",
        control_enabled = false,
        source = {
            kind = "MidiRaw",
            pattern = "B0 3"..ch.." [0001 dcba]",
        },
        glue = {
            source_interval = {0.05, 0.75},
        },
        target = {
            kind = "Virtual",
            id = prefix.."v-pot/boost-cut",
        },
    }
    local v_pot_feedback_single = {
        id = prefix.."v-pot/single",
        group = "v-pot-leds",
        control_enabled = false,
        source = {
            kind = "MidiRaw",
            pattern = "B0 3"..ch.." [0000 dcba]",
        },
        glue = {
            source_interval = {0, 0.75},
        },
        target = {
            kind = "Virtual",
            id = prefix.."v-pot/single",
        },
    }
    local v_pot_feedback_spread = {
        id = prefix.."v-pot/spread",
        group = "v-pot-leds",
        control_enabled = false,
        source = {
            kind = "MidiRaw",
            pattern = "B0 3"..ch.." [0011 dcba]",
        },
        glue = {
            source_interval = {0, 0.4},
        },
        target = {
            kind = "Virtual",
            id = prefix.."v-pot/spread",
        },
    }
    local mute = {
        id = prefix.."mute",
        group = "mute",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 16 + ch,
        },
        target = {
            kind = "Virtual",
            id = prefix.."mute",
            character = "Button",
        },
    }
    local solo = {
        id = prefix.."solo",
        group = "solo",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 8 + ch,
        },
        target = {
            kind = "Virtual",
            id = prefix.."solo",
            character = "Button",
        },
    }
    local record_ready = {
        id = prefix.."record-ready",
        group = "record-ready",
        source = {
            kind = "MidiNoteVelocity",
            channel = 0,
            key_number = 0 + ch,
        },
        target = {
            kind = "Virtual",
            id = prefix.."record-ready",
            character = "Button",
        },
    }
    local lcd_line1 = {
        id = prefix.."lcd/line1",
        group = "lcd",
        control_enabled = false,
        source = {
            kind = lcd_kind,
            channel = ch,
            line = 0,
        },
        target = {
            kind = "Virtual",
            id = prefix.."lcd/line1",
        },
    }
    local lcd_line2 = {
        id = prefix.."lcd/line2",
        group = "lcd",
        control_enabled = false,
        source = {
            kind = lcd_kind,
            channel = ch,
            line = 1,
        },
        target = {
            kind = "Virtual",
            id = prefix.."lcd/line2",
        },
    }
    local meter = {
        id = prefix.."meter/peak",
        group = "meter",
        control_enabled = false,
        source = {
            kind = "MidiRaw",
            pattern = "D0 [0"..binary_eight[ch].." dcba]",
        },
        target = {
            kind = "Virtual",
            id = prefix.."meter/peak",
        },
    }
    table.insert(mappings, v_select)
    table.insert(mappings, fader_touch)
    table.insert(mappings, select)
    table.insert(mappings, fader)
    table.insert(mappings, v_pot_control)
    table.insert(mappings, v_pot_feedback_default)
    table.insert(mappings, v_pot_feedback_wrap)
    table.insert(mappings, v_pot_feedback_boost_cut)
    table.insert(mappings, v_pot_feedback_single)
    table.insert(mappings, v_pot_feedback_spread)
    table.insert(mappings, mute)
    table.insert(mappings, solo)
    table.insert(mappings, record_ready)
    table.insert(mappings, lcd_line1)
    table.insert(mappings, lcd_line2)
    table.insert(mappings, meter)
end

return {
    kind = "ControllerCompartment",
    value = {
        groups = groups,
        mappings = mappings
    },
}