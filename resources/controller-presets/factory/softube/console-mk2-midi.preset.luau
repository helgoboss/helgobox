--- name: Console 1 mk2
--- realearn_version: 2.16.12
--- author: helgoboss
--- description: |
---     This controller preset implements support for the Softube Console 1 controller in MIDI mode.
--- setup_instructions: |
---     Simply connect the controller. Ensure that the Console 1 On-Screen Display software is not started!
--- device_manufacturer: Softube
--- device_name: Console 1 mk2
--- provided_schemes: [softube/console1-mk2]

--!strict

-- Config

-- With this, we can simulate true relative control.
--
-- In MIDI mode, the encoders of the Console 1 send absolute control values, not relative ones. This is 
-- not optimal because it means we can't define step sizes, wrap etc. However, as long as feedback is enabled, at least we don't
-- need to suffer from parameter jumps (a common disadvantage of absolute control). 
-- This is because the Console 1 doesn't just use the feedback to set the LED ring, it also resets its internal encoder 
-- value to the incoming feedback value.
--
-- If we simulate relative control, we get the advantages of relative control. However, it doesn't
-- work so nicely with feedback :( The "reset-internal-encoder-value" mechanism mentioned above interferes.
-- Without further treatment, this makes the control stuck. We can make control work by not sending echo feedback
-- but then the indicated LED ring value is incorrect when we turn the encoder (however, it is correct when changing the parameter
-- in REAPER itself - as long as we don't disable feedback completely).
-- The technique we use right now is to compare with the previously sent feedback value - if available - and otherwise the previously
-- sent control value. This sort of works, but there can be glitches because it's not always sure that the last-sent feedback value
-- has actually been received by the controller.
local simulate_relative_control = true

local native_mode = true

-- Code

local realearn = require("realearn")

local abs_to_rel_transformation = [[
y_type = 1;
-- If the last-sent feedback value is available, we use this one as reference (because the controller
-- resets its internal value to the last-sent feedback value). If not (e.g. if feedback is disabled),
-- we use the last-sent control value as reference.
ref = last_feedback_value == prev_last_feedback_value ? prev_x : last_feedback_value;
y = count == 0 ? (
    -- On first invocation, we don't have any reference value to compare to
    0
) : x > ref || x == 1 ? (
    -- Controller sends higher number than reference value or encoder hit right boundary. Looks like an increment.
    1
) : x < ref || x == 0 ? (
    -- Controller sends lower number than reference value or encoder hit left boundary. Looks like a decrement.
    -1
) : (
    -- No change
    none
);
prev_last_feedback_value = last_feedback_value;
prev_x = x;
count += 1;
]]

local function button(id: string, name: string, cc: number): realearn.Mapping
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

local function encoder(id: string, name: string, cc: number): realearn.Mapping
    return realearn.Mapping {
        id = id,
        name = name,
        source = realearn.Source.MidiControlChangeValue {
            channel = 0,
            controller_number = cc,
            character = "Range",
            -- Preventing echo feedback would make the LED ring show the incorrect value when we turn the encoder
            -- (it would be correct when changing the parameter in REAPER itself).
            -- feedback_behavior = "PreventEchoFeedback",
        },
        glue = realearn.Glue {
            control_transformation = if simulate_relative_control then abs_to_rel_transformation else nil
        },
        target = realearn.Target.Virtual {
            id = id,
            character = "Multi",
        },
    }
end

local function meter(id: string, name: string, cc: number): realearn.Mapping
    return realearn.Mapping {
        id = id,
        name = name,
        control_enabled = false,
        source = realearn.Source.MidiControlChangeValue {
            channel = 0,
            controller_number = cc,
            character = "Range",
        },
        target = realearn.Target.Virtual {
            id = id,
            character = "Multi",
        },
    }
end


local mappings = {
    -- Buttons in row 1
    button("display-on", "Display: On", 102),
    button("display-mode", "Display: Mode", 104),
    button("bank-left", "Page: -", 97),
    button("bank-right", "Page: +", 96),
    button("track-group", "Track: Group", 123),
    button("track-copy", "Track: Copy", 120),
    button("order", "Order", 14),
    button("external-sidechain", "External Sidechain", 17),
    -- Buttons in row 2
    button("shape", "Shape", 53),
    button("eq", "Equalizer", 80),
    button("comp", "Compressor", 46),
    -- Buttons in row 3
    button("eq/low-type", "EQ: Low type", 93),
    button("eq/high-type", "EQ: High type", 65),
    -- Buttons in row 4
    button("shape/hard-gate", "Shape: Hard Gate", 59),
    button("solo", "Solo", 13),
    button("mute", "Mute", 12),
    -- Remaining buttons
    button("filters-to-compressor", "Filters to Compressor", 61),
    button("phase-inv", "Phase Inv.", 108),
    button("preset", "Preset", 58),
    -- Encoders in row 1
    encoder("high-cut", "High Cut", 105),
    encoder("shape/gate-release", "Shape: Gate Release", 56),
    encoder("eq/low-mid-type", "EQ: Low Mid Type", 90),
    encoder("eq/high-mid-type", "EQ: High Mid Type", 87),
    encoder("comp/attack", "Comp: Attack", 51),
    encoder("drive", "Drive", 15),
    -- Encoders in row 2
    encoder("shape/gate", "Shape: Gate", 54),
    encoder("comp/ratio", "Comp: Ratio", 49),
    -- Encoders in row 3
    encoder("low-cut", "Low Cut", 103),
    encoder("shape/sustain", "Shape: Sustain", 55),
    encoder("eq/low-freq", "EQ: Low Frequency", 92),
    encoder("eq/low-mid-freq", "EQ: Low Mid Frequency", 89),
    encoder("eq/high-mid-freq", "EQ: High Mid Frequency", 86),
    encoder("eq/high-freq", "EQ: High Frequency", 83),
    encoder("comp/release", "Comp: Release", 48),
    encoder("character", "Character", 18),
    -- Encoders in row 4
    encoder("input/gain", "Input Gain", 107),
    encoder("comp/parallel-dry-wet", "Comp: Parallel Dry/Wet", 50),
    -- Encoders in row 5
    encoder("shape/punch", "Shape: Punch", 57),
    encoder("eq/low/gain", "EQ: Low Gain", 91),
    encoder("eq/low-mid/gain", "EQ: Low Mid Gain", 88),
    encoder("eq/high-mid/gain", "EQ: High Mid Gain", 85),
    encoder("eq/high/gain", "EQ: High Gain", 82),
    encoder("comp/threshold", "Comp: Threshold", 47),
    encoder("pan", "Pan", 10),
    encoder("output/volume", "Volume", 7),
    -- Meters 
    meter("input/meter/left", "Input Meter: Left channel", 110),
    meter("input/meter/right", "Input Meter: Right channel", 111),
    meter("output/meter/left", "Output Meter: Left channel", 112),
    meter("output/meter/right", "Output Meter: Right channel", 113),
    meter("shape/meter", "Shape Meter", 114),
    meter("comp/meter", "Compressor Meter", 115),
}

-- "Select track" buttons (simply exposed as numbered buttons)
for i=0,19 do
    local id = `select-track-{i + 1}`
    local m = realearn.Mapping {
        id = id,
        name = `Select track {i + 1}`,
        source = realearn.Source.MidiControlChangeValue {
            channel = 0,
            controller_number = 21 + i,
            character = "Button",
        },
        target = realearn.Target.Virtual {
            id = i,
            character = "Button",
        },
    }
    table.insert(mappings, m)
end

return realearn.Compartment {
    mappings = mappings,
}