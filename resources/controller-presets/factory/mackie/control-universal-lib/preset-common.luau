--!strict

local realearn = require("realearn")

local module = {}

export type McuPresetConfig = {
    companion_data: any,
    support_x_touch_colors: boolean,
}

function module.create_compartment(config: McuPresetConfig): realearn.Compartment
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

    local groups: { realearn.Group } = {
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

    local mappings: { realearn.Mapping } = {
        {
            id = "main/fader",
            group = "fader",
            source = realearn.Source.MidiPitchBendChangeValue {
                channel = 8,
            },
            target = realearn.Target.Virtual {
                id = "main/fader",
            },
        },
        {
            id = "jog",
            feedback_enabled = false,
            source = realearn.Source.MidiControlChangeValue {
                channel = 0,
                controller_number = 60,
                character = "Relative1",
                fourteen_bit = false,
            },
            target = realearn.Target.Virtual {
                id = "jog",
            },
        },
        {
            id = "main/fader/touch",
            group = "fader-touch",
            feedback_enabled = false,
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 112,
            },
            target = realearn.Target.Virtual {
                id = "main/fader/touch",
                character = "Button",
                learnable = false,
            },
        },
        {
            id = "marker",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 84,
            },
            target = realearn.Target.Virtual {
                id = "marker",
                character = "Button",
            },
        },
        {
            id = "read",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 74,
            },
            target = realearn.Target.Virtual {
                id = "read",
                character = "Button",
            },
        },
        {
            id = "write",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 75,
            },
            target = realearn.Target.Virtual {
                id = "write",
                character = "Button",
            },
        },
        {
            id = "ch-left",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 48,
            },
            target = realearn.Target.Virtual {
                id = "ch-left",
                character = "Button",
            },
        },
        {
            id = "ch-right",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 49,
            },
            target = realearn.Target.Virtual {
                id = "ch-right",
                character = "Button",
            },
        },
        {
            id = "bank-left",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 46,
            },
            target = realearn.Target.Virtual {
                id = "bank-left",
                character = "Button",
            },
        },
        {
            id = "bank-right",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 47,
            },
            target = realearn.Target.Virtual {
                id = "bank-right",
                character = "Button",
            },
        },
        {
            id = "rewind",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 91,
            },
            target = realearn.Target.Virtual {
                id = "rewind",
                character = "Button",
            },
        },
        {
            id = "fast-fwd",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 92,
            },
            target = realearn.Target.Virtual {
                id = "fast-fwd",
                character = "Button",
            },
        },
        {
            id = "play",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 94,
            },
            target = realearn.Target.Virtual {
                id = "play",
                character = "Button",
            },
        },
        {
            id = "stop",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 93,
            },
            target = realearn.Target.Virtual {
                id = "stop",
                character = "Button",
            },
        },
        {
            id = "record",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 95,
            },
            target = realearn.Target.Virtual {
                id = "record",
                character = "Button",
            },
        },
        {
            id = "cycle",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 86,
            },
            target = realearn.Target.Virtual {
                id = "cycle",
                character = "Button",
            },
        },
        {
            id = "zoom",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 100,
            },
            target = realearn.Target.Virtual {
                id = "zoom",
                character = "Button",
            },
        },
        {
            id = "scrub",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 101,
            },
            target = realearn.Target.Virtual {
                id = "scrub",
                character = "Button",
            },
        },
        {
            id = "cursor-left",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 98,
            },
            target = realearn.Target.Virtual {
                id = "cursor-left",
                character = "Button",
            },
        },
        {
            id = "cursor-right",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 99,
            },
            target = realearn.Target.Virtual {
                id = "cursor-right",
                character = "Button",
            },
        },
        {
            id = "cursor-up",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 96,
            },
            target = realearn.Target.Virtual {
                id = "cursor-up",
                character = "Button",
            },
        },
        {
            id = "cursor-down",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 97,
            },
            target = realearn.Target.Virtual {
                id = "cursor-down",
                character = "Button",
            },
        },
        {
            id = "nudge",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 85,
            },
            target = realearn.Target.Virtual {
                id = "nudge",
                character = "Button",
            },
        },
        {
            id = "drop",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 87,
            },
            target = realearn.Target.Virtual {
                id = "drop",
                character = "Button",
            },
        },
        {
            id = "replace",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 88,
            },
            target = realearn.Target.Virtual {
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
            target = realearn.Target.Virtual {
                id = "click",
                character = "Button",
            },
        },
        {
            id = "solo",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 90,
            },
            target = realearn.Target.Virtual {
                id = "solo",
                character = "Button",
            },
        },
        {
            id = "f1",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 54,
            },
            target = realearn.Target.Virtual {
                id = "f1",
                character = "Button",
            },
        },
        {
            id = "f2",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 55,
            },
            target = realearn.Target.Virtual {
                id = "f2",
                character = "Button",
            },
        },
        {
            id = "f3",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 56,
            },
            target = realearn.Target.Virtual {
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
            target = realearn.Target.Virtual {
                id = "f4",
                character = "Button",
            },
        },
        {
            id = "f5",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 58,
            },
            target = realearn.Target.Virtual {
                id = "f5",
                character = "Button",
            },
        },
        {
            id = "f6",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 59,
            },
            target = realearn.Target.Virtual {
                id = "f6",
                character = "Button",
            },
        },
        {
            id = "smpte-beats",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 53,
            },
            target = realearn.Target.Virtual {
                id = "smpte-beats",
                character = "Button",
            },
        },
        {
            id = "lcd/assignment",
            group = "lcd",
            control_enabled = false,
            source = realearn.Source.MackieSevenSegmentDisplay {
            },
            target = realearn.Target.Virtual {
                id = "lcd/assignment",
            },
        },
        {
            id = "lcd/timecode",
            group = "lcd",
            control_enabled = false,
            source = realearn.Source.MackieSevenSegmentDisplay {
                scope = "Tc",
            },
            target = realearn.Target.Virtual {
                id = "lcd/timecode",
            },
        },
    }

    -- For each channel
    for ch = 0, channel_count - 1 do
        local human_ch = ch + 1
        local prefix = "ch"..human_ch.."/"
        local v_select = realearn.Mapping {
            id = prefix.."v-select",
            group = "v-select",
            feedback_enabled = false,
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 32 + ch,
            },
            target = realearn.Target.Virtual {
                id = prefix.."v-select",
                character = "Button",
            },
        }
        local fader_touch = realearn.Mapping {
            id = prefix.."fader/touch",
            group = "fader-touch",
            feedback_enabled = false,
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 104 + ch,
            },
            target = realearn.Target.Virtual {
                id = prefix.."fader/touch",
                character = "Button",
                learnable = false,
            },
        }
        local select = realearn.Mapping {
            id = prefix.."select",
            group = "select",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 24 + ch,
            },
            target = realearn.Target.Virtual {
                id = prefix.."select",
                character = "Button",
            },
        }
        local fader = realearn.Mapping {
            id = prefix.."fader",
            group = "fader",
            source = realearn.Source.MidiPitchBendChangeValue {
                channel = ch,
            },
            target = realearn.Target.Virtual {
                id = prefix.."fader",
            },
        }
        local v_pot_control = realearn.Mapping {
            id = prefix.."v-pot/control",
            group = "v-pot",
            feedback_enabled = false,
            source = realearn.Source.MidiControlChangeValue {
                channel = 0,
                controller_number = 16 + ch,
                character = "Relative3",
                fourteen_bit = false,
            },
            glue = {
                step_factor_interval = {1, 100},
            },
            target = realearn.Target.Virtual {
                id = prefix.."v-pot",
            },
        }
        local v_pot_feedback_default = realearn.Mapping {
            id = prefix.."v-pot/feedback",
            group = "v-pot-leds",
            control_enabled = false,
            source = realearn.Source.MidiRaw {
                pattern = "B0 3"..ch.." [0000 dcba]",
            },
            glue = {
                source_interval = {0, 0.75},
            },
            target = realearn.Target.Virtual {
                id = prefix.."v-pot",
            },
        }
        local v_pot_feedback_wrap = realearn.Mapping {
            id = prefix.."v-pot/wrap",
            group = "v-pot-leds",
            control_enabled = false,
            source = realearn.Source.MidiRaw {
                pattern = "B0 3"..ch.." [0010 dcba]",
            },
            glue = {
                source_interval = {0, 0.75},
            },
            target = realearn.Target.Virtual {
                id = prefix.."v-pot/wrap",
            },
        }
        local v_pot_feedback_boost_cut = realearn.Mapping {
            id = prefix.."v-pot/boost-cut",
            group = "v-pot-leds",
            control_enabled = false,
            source = realearn.Source.MidiRaw {
                pattern = "B0 3"..ch.." [0001 dcba]",
            },
            glue = {
                source_interval = {0.05, 0.75},
            },
            target = realearn.Target.Virtual {
                id = prefix.."v-pot/boost-cut",
            },
        }
        local v_pot_feedback_single = realearn.Mapping {
            id = prefix.."v-pot/single",
            group = "v-pot-leds",
            control_enabled = false,
            source = realearn.Source.MidiRaw {
                pattern = "B0 3"..ch.." [0000 dcba]",
            },
            glue = {
                source_interval = {0, 0.75},
            },
            target = realearn.Target.Virtual {
                id = prefix.."v-pot/single",
            },
        }
        local v_pot_feedback_spread = realearn.Mapping {
            id = prefix.."v-pot/spread",
            group = "v-pot-leds",
            control_enabled = false,
            source = realearn.Source.MidiRaw {
                pattern = "B0 3"..ch.." [0011 dcba]",
            },
            glue = {
                source_interval = {0, 0.4},
            },
            target = realearn.Target.Virtual {
                id = prefix.."v-pot/spread",
            },
        }
        local mute = realearn.Mapping {
            id = prefix.."mute",
            group = "mute",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 16 + ch,
            },
            target = {
                kind = "Virtual",
                id = prefix.."mute",
                character = "Button",
            },
        }
        local solo = realearn.Mapping {
            id = prefix.."solo",
            group = "solo",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 8 + ch,
            },
            target = realearn.Target.Virtual {
                id = prefix.."solo",
                character = "Button",
            },
        }
        local record_ready = realearn.Mapping {
            id = prefix.."record-ready",
            group = "record-ready",
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = 0 + ch,
            },
            target = realearn.Target.Virtual {
                id = prefix.."record-ready",
                character = "Button",
            },
        }
        local normal_lcd_line_1 = realearn.Source.MackieLcd {
            channel = ch,
            line = 0,
        }
        local x_touch_lcd_line_1 = realearn.Source.XTouchMackieLcd {
            channel = ch,
            line = 0,
        }
        local lcd_line1 = realearn.Mapping {
            id = prefix.."lcd/line1",
            group = "lcd",
            control_enabled = false,
            source = if config.support_x_touch_colors then x_touch_lcd_line_1 else normal_lcd_line_1,
            target = realearn.Target.Virtual {
                id = prefix.."lcd/line1",
            },
        }
        local normal_lcd_line_2 = realearn.Source.MackieLcd {
            channel = ch,
            line = 1,
        }
        local x_touch_lcd_line_2 = realearn.Source.XTouchMackieLcd {
            channel = ch,
            line = 1,
        }
        local lcd_line2 = realearn.Mapping {
            id = prefix.."lcd/line2",
            group = "lcd",
            control_enabled = false,
            source = if config.support_x_touch_colors then x_touch_lcd_line_2 else normal_lcd_line_2,
            target = realearn.Target.Virtual {
                id = prefix.."lcd/line2",
            },
        }
        local meter = realearn.Mapping {
            id = prefix.."meter/peak",
            group = "meter",
            control_enabled = false,
            source = realearn.Source.MidiRaw {
                pattern = "D0 [0"..binary_eight[ch].." dcba]",
            },
            target = realearn.Target.Virtual {
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
    
    return realearn.Compartment {
        groups = groups,
        mappings = mappings,
        custom_data = {
            companion = config.companion_data,
        },
    }
end

return module
