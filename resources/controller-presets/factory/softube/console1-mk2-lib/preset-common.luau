--!strict

local realearn = require("realearn")

local module = {}

export type PresetMode = "native" | "midi-absolute" | "midi-relative"

export type PresetConfig = {
    mode: PresetMode,
    
}

function module.create_compartment(config: PresetConfig): realearn.Compartment
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
    local simulate_relative_control_in_midi_mode = config.mode == "midi-relative"

    local native_mode = config.mode == "native"

    -- Code

    local realearn = require("realearn")

    local sysex_to_relative_transformation = [[
y_type = 1;
y = prev_timestamp == 0 ? (
    1
) : (
    // Implement encoder acceleration based on timestamp diffs
    diff_millis = (realearn_timestamp - prev_timestamp) * 1000;
    added = 10 / diff_millis;
    // realearn_dbg(added);
    1 + added
);
prev_timestamp = realearn_timestamp;
]]

    local abs_to_rel_transformation = [[
y_type = 1;
// If the last-sent feedback value is available, we use this one as reference (because the controller
// resets its internal value to the last-sent feedback value). If not (e.g. if feedback is disabled),
// we use the last-sent control value as reference.
ref = realearn_last_feedback_value == prev_last_feedback_value ? prev_x : realearn_last_feedback_value;
y = count == 0 ? (
    // On first invocation, we don't have any reference value to compare to
    0
) : x > ref || x == 1 ? (
    // Controller sends higher number than reference value or encoder hit right boundary. Looks like an increment.
    1
) : x < ref || x == 0 ? (
    // Controller sends lower number than reference value or encoder hit left boundary. Looks like a decrement.
    -1
) : (
    // No change
    none
);
prev_last_feedback_value = last_feedback_value;
prev_x = x;
count += 1;
]]

    local mappings = {}

    local function button(
        id: string,
        target: realearn.Target,
        name: string,
        cc: number?,
        on_sysex: string,
        off_sysex: string
    )
        local cc_source = realearn.Source.MidiControlChangeValue {
            channel = 0,
            controller_number = cc,
            character = "Button",
        }
        if native_mode then
            local on_mapping = realearn.Mapping {
                id = `{id}-on`,
                feedback_enabled = false,
                name = name,
                source = realearn.Source.MidiRaw {
                    pattern = on_sysex,
                },
                target = target,
            }
            table.insert(mappings, on_mapping)
            local off_mapping = realearn.Mapping {
                id = `{id}-off`,
                feedback_enabled = false,
                name = name,
                source = realearn.Source.MidiRaw {
                    pattern = off_sysex,
                },
                glue = {
                    reverse = true,
                },
                target = target,
            }
            table.insert(mappings, off_mapping)
            if cc then
                local feedback_mapping = realearn.Mapping {
                    id = `{id}-feedback`,
                    control_enabled = false,
                    name = name,
                    source = cc_source,
                    target = target,
                }
                table.insert(mappings, feedback_mapping)
            end
        else
            local mapping = realearn.Mapping {
                id = id,
                name = name,
                source = cc_source,
                target = target,
            }
            table.insert(mappings, mapping)
        end
    end

    local function named_button(id: string, name: string, cc: number?, on_sysex: string, off_sysex: string)
        local target = realearn.Target.Virtual {
            id = id,
            character = "Button",
        }
        button(id, target, name, cc, on_sysex, off_sysex)
    end

    local function numbered_button(index: number, name: string, cc: number, on_sysex: string, off_sysex: string)
        local target = realearn.Target.Virtual {
            id = index,
            character = "Button",
        }
        button(tostring(index), target, name, cc, on_sysex, off_sysex)
    end

    local function encoder(id: string, name: string, cc: number, cw_sysex: string, ccw_sysex: string)
        local cc_source = realearn.Source.MidiControlChangeValue {
            channel = 0,
            controller_number = cc,
            character = "Range",
            -- Preventing echo feedback would make the LED ring show the incorrect value when we turn the encoder
            -- (it would be correct when changing the parameter in REAPER itself).
            -- feedback_behavior = "PreventEchoFeedback",
        }
        local target = realearn.Target.Virtual {
            id = id,
            character = "Multi",
        }
        if native_mode then
            local cw_mapping = realearn.Mapping {
                id = `{id}-cw`,
                feedback_enabled = false,
                name = name,
                source = realearn.Source.MidiRaw {
                    pattern = cw_sysex,
                    character = "Button",
                },
                glue = realearn.Glue {
                    control_transformation = sysex_to_relative_transformation,
                    step_factor_interval = {1, 100},
                },
                target = target,
            }
            local ccw_mapping = realearn.Mapping {
                id = `{id}-ccw`,
                feedback_enabled = false,
                name = name,
                source = realearn.Source.MidiRaw {
                    pattern = ccw_sysex,
                    character = "Button",
                },
                glue = {
                    control_transformation = sysex_to_relative_transformation,
                    step_factor_interval = {1, 100},
                    reverse = true,
                },
                target = target,
            }
            local feedback_mapping = realearn.Mapping {
                id = `{id}-feedback`,
                control_enabled = false,
                name = name,
                source = cc_source,
                target = target,
            }
            table.insert(mappings, cw_mapping)
            table.insert(mappings, ccw_mapping)
            table.insert(mappings, feedback_mapping)
        else
            local mapping = realearn.Mapping {
                id = id,
                name = name,
                source = cc_source,
                glue = realearn.Glue {
                    control_transformation = if simulate_relative_control_in_midi_mode then abs_to_rel_transformation else nil,
                },
                target = target,
            }
            table.insert(mappings, mapping)
        end
    end

    local function meter(id: string, name: string, cc: number)
        local m = realearn.Mapping {
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
        table.insert(mappings, m)
    end

    -- Named buttons in row 1
    named_button("display-on", "Display: On", 102, "", "")
    named_button(
        "display/mode",
        "Display: Mode",
        104,
        "F0 7D 00 00 00 00 02 00 01 7F 01 02 55 0A 0C F7",
        "F0 7D 00 00 00 00 02 00 01 00 41 38 39 27 01 F7"
    )
    named_button(
        "page/prev",
        "Page: -",
        97,
        "F0 7D 00 00 00 00 02 00 03 7F 33 34 37 08 0D F7",
        "F0 7D 00 00 00 00 02 00 03 00 73 0E 5B 25 00 F7"
    )
    named_button(
        "page/next",
        "Page: +",
        96,
        "F0 7D 00 00 00 00 02 00 04 7F 7C 75 21 4F 0E F7",
        "F0 7D 00 00 00 00 02 00 04 00 3C 4F 4D 62 03 F7"
    )
    named_button(
        "track/group",
        "Track: Group",
        123,
        "F0 7D 00 00 00 00 02 00 19 7F 03 19 4D 53 06 F7",
        "F0 7D 00 00 00 00 02 00 19 00 43 23 21 7E 0B F7"
    )
    named_button(
        "track/copy",
        "Track: Copy",
        120,
        "F0 7D 00 00 00 00 02 00 1A 7F 28 34 1E 10 07 F7",
        "F0 7D 00 00 00 00 02 00 1A 00 68 0E 72 3D 0A F7"
    )
    named_button(
        "order",
        "Order",
        14,
        "F0 7D 00 00 00 00 02 00 3A 7F 3D 30 3A 32 0A F7",
        "F0 7D 00 00 00 00 02 00 3A 00 7D 0A 56 1F 07 F7"
    )
    named_button(
        "external-sidechain",
        "External Sidechain",
        17,
        "F0 7D 00 00 00 00 02 00 3B 7F 24 2B 0B 73 0A F7",
        "F0 7D 00 00 00 00 02 00 3B 00 64 11 67 5E 07 F7"
    )
    -- Numbered buttons in row 1
    numbered_button(
        0,
        `Select track 1`,
        21,
        "F0 7D 00 00 00 00 02 00 05 7F 65 6E 10 0E 0E F7",
        "F0 7D 00 00 00 00 02 00 05 00 25 54 7C 23 03 F7"
    )
    numbered_button(
        1,
        `Select track 2`,
        22,
        "F0 7D 00 00 00 00 02 00 06 7F 4E 43 43 4D 0F F7",
        "F0 7D 00 00 00 00 02 00 06 00 0E 79 2F 60 02 F7"
    )
    numbered_button(
        2,
        `Select track 3`,
        23,
        "F0 7D 00 00 00 00 02 00 07 7F 57 58 72 0C 0F F7",
        "F0 7D 00 00 00 00 02 00 07 00 17 62 1E 21 02 F7"
    )
    numbered_button(
        3,
        `Select track 4`,
        24,
        "F0 7D 00 00 00 00 02 00 08 7F 50 40 6E 43 02 F7",
        "F0 7D 00 00 00 00 02 00 08 00 10 7A 02 6E 0F F7"
    )
    numbered_button(
        4,
        `Select track 5`,
        25,
        "F0 7D 00 00 00 00 02 00 09 7F 49 5B 5F 02 02 F7",
        "F0 7D 00 00 00 00 02 00 09 00 09 61 33 2F 0F F7"
    )
    numbered_button(
        5,
        `Select track 6`,
        26,
        "F0 7D 00 00 00 00 02 00 0A 7F 62 76 0C 41 03 F7",
        "F0 7D 00 00 00 00 02 00 0A 00 22 4C 60 6C 0E F7"
    )
    numbered_button(
        6,
        "Select track 7",
        27,
        "F0 7D 00 00 00 00 02 00 0B 7F 7B 6D 3D 00 03 F7",
        "F0 7D 00 00 00 00 02 00 0B 00 3B 57 51 2D 0E F7"
    )
    numbered_button(
        7,
        "Select track 8",
        28,
        "F0 7D 00 00 00 00 02 00 0C 7F 34 2C 2B 47 00 F7",
        "F0 7D 00 00 00 00 02 00 0C 00 74 16 47 6A 0D F7"
    )
    numbered_button(
        8,
        "Select track 9",
        29,
        "F0 7D 00 00 00 00 02 00 0D 7F 2D 37 1A 06 00 F7",
        "F0 7D 00 00 00 00 02 00 0D 00 6D 0D 76 2B 0D F7"
    )
    numbered_button(
        9,
        "Select track 10",
        30,
        "F0 7D 00 00 00 00 02 00 0E 7F 06 1A 49 45 01 F7",
        "F0 7D 00 00 00 00 02 00 0E 00 46 20 25 68 0C F7"
    )
    numbered_button(
        10,
        "Select track 11",
        31,
        "F0 7D 00 00 00 00 02 00 0F 7F 1F 01 78 04 01 F7",
        "F0 7D 00 00 00 00 02 00 0F 00 5F 3B 14 29 0C F7"
    )
    numbered_button(
        11,
        "Select track 12",
        32,
        "F0 7D 00 00 00 00 02 00 10 7F 52 5B 76 1A 08 F7",
        "F0 7D 00 00 00 00 02 00 10 00 12 61 1A 37 05 F7"
    )
    numbered_button(
        12,
        "Select track 13",
        33,
        "F0 7D 00 00 00 00 02 00 11 7F 4B 40 47 5B 08 F7",
        "F0 7D 00 00 00 00 02 00 11 00 0B 7A 2B 76 05 F7"
    )
    numbered_button(
        13,
        "Select track 14",
        34,
        "F0 7D 00 00 00 00 02 00 12 7F 60 6D 14 18 09 F7",
        "F0 7D 00 00 00 00 02 00 12 00 20 57 78 35 04 F7"
    )
    numbered_button(
        14,
        "Select track 15",
        35,
        "F0 7D 00 00 00 00 02 00 13 7F 79 76 25 59 09 F7",
        "F0 7D 00 00 00 00 02 00 13 00 39 4C 49 74 04 F7"
    )
    numbered_button(
        15,
        "Select track 16",
        36,
        "F0 7D 00 00 00 00 02 00 14 7F 36 37 33 1E 0A F7",
        "F0 7D 00 00 00 00 02 00 14 00 76 0D 5F 33 07 F7"
    )
    numbered_button(
        16,
        "Select track 17",
        37,
        "F0 7D 00 00 00 00 02 00 15 7F 2F 2C 02 5F 0A F7",
        "F0 7D 00 00 00 00 02 00 15 00 6F 16 6E 72 07 F7"
    )
    numbered_button(
        17,
        "Select track 18",
        38,
        "F0 7D 00 00 00 00 02 00 16 7F 04 01 51 1C 0B F7",
        "F0 7D 00 00 00 00 02 00 16 00 44 3B 3D 31 06 F7"
    )
    numbered_button(
        18,
        "Select track 19",
        39,
        "F0 7D 00 00 00 00 02 00 17 7F 1D 1A 60 5D 0B F7",
        "F0 7D 00 00 00 00 02 00 17 00 5D 20 0C 70 06 F7"
    )
    numbered_button(
        19,
        "Select track 20",
        40,
        "F0 7D 00 00 00 00 02 00 18 7F 1A 02 7C 12 06 F7",
        "F0 7D 00 00 00 00 02 00 18 00 5A 38 10 3F 0B F7"
    )
    -- Buttons in row 2
    named_button(
        "shape",
        "Shape",
        53,
        "F0 7D 00 00 00 00 02 00 21 7F 14 06 71 28 01 F7",
        "F0 7D 00 00 00 00 02 00 21 00 54 3C 1D 05 0C F7"
    )
    named_button(
        "eq",
        "Equalizer",
        80,
        "F0 7D 00 00 00 00 02 00 27 7F 42 5C 56 2E 02 F7",
        "F0 7D 00 00 00 00 02 00 27 00 02 66 3A 03 0F F7"
    )
    named_button(
        "comp",
        "Compressor",
        46,
        "F0 7D 00 00 00 00 02 00 34 7F 23 33 17 3C 07 F7",
        "F0 7D 00 00 00 00 02 00 34 00 63 09 7B 11 0A F7"
    )
    -- Buttons in row 3
    named_button(
        "eq/low/type",
        "EQ: Low type",
        93,
        "F0 7D 00 00 00 00 02 00 28 7F 45 44 4A 61 0F F7",
        "F0 7D 00 00 00 00 02 00 28 00 05 7E 26 4C 02 F7"
    )
    named_button(
        "eq/high/type",
        "EQ: High type",
        65,
        "F0 7D 00 00 00 00 02 00 31 7F 5E 44 63 79 05 F7",
        "F0 7D 00 00 00 00 02 00 31 00 1E 7E 0F 54 08 F7"
    )
    -- Buttons in row 4
    named_button(
        "shape/hard-gate",
        "Shape: Hard Gate",
        59,
        "F0 7D 00 00 00 00 02 00 23 7F 26 30 13 2A 00 F7",
        "F0 7D 00 00 00 00 02 00 23 00 66 0A 7F 07 0D F7"
    )
    named_button(
        "solo",
        "Solo",
        13,
        "F0 7D 00 00 00 00 02 00 3F 7F 40 47 4E 77 08 F7",
        "F0 7D 00 00 00 00 02 00 3F 00 00 7D 22 5A 05 F7"
    )
    named_button(
        "mute",
        "Mute",
        12,
        "F0 7D 00 00 00 00 02 00 40 7F 68 60 2B 4E 04 F7",
        "F0 7D 00 00 00 00 02 00 40 00 28 5A 47 63 09 F7"
    )
    -- Remaining buttons
    named_button(
        "filters-to-compressor",
        "Filters to Compressor",
        61,
        "F0 7D 00 00 00 00 02 00 1E 7F 4C 58 5B 14 05 F7",
        "F0 7D 00 00 00 00 02 00 1E 00 0C 62 37 39 08 F7"
    )
    named_button(
        "phase-inv",
        "Phase Inv.",
        108,
        "F0 7D 00 00 00 00 02 00 1F 7F 55 43 6A 55 05 F7",
        "F0 7D 00 00 00 00 02 00 1F 00 15 79 06 78 08 F7"
    )
    named_button(
        "preset",
        "Preset",
        58,
        "F0 7D 00 00 00 00 02 00 20 7F 0D 1D 40 69 01 F7",
        "F0 7D 00 00 00 00 02 00 20 00 4D 27 2C 44 0C F7"
    )
    if native_mode then
        named_button(
            "fine-adjust",
            "Fine Adjust",
            nil,
            "F0 7D 00 00 00 00 02 00 02 7F 2A 2F 06 49 0D F7",
            "F0 7D 00 00 00 00 02 00 02 00 6A 15 6A 64 00 F7"
        )
    end
    -- Encoders in row 1
    encoder(
        "high-cut",
        "High Cut",
        105,
        "F0 7D 00 00 00 00 02 00 1C 7F 7E 6E 39 16 04 F7",
        "F0 7D 00 00 00 00 02 00 1C 00 3E 54 55 3B 09 F7"
    )
    encoder(
        "shape/gate-release",
        "Shape: Gate Release",
        56,
        "F0 7D 00 00 00 00 02 00 24 7F 69 71 05 6D 03 F7",
        "F0 7D 00 00 00 00 02 00 24 00 29 4B 69 40 0E F7"
    )
    encoder(
        "eq/low-mid/type",
        "EQ: Low Mid Type",
        90,
        "F0 7D 00 00 00 00 02 00 2C 7F 21 28 0F 65 0D F7",
        "F0 7D 00 00 00 00 02 00 2C 00 61 12 63 48 00 F7"
    )
    encoder(
        "eq/high-mid/type",
        "EQ: High Mid Type",
        87,
        "F0 7D 00 00 00 00 02 00 2F 7F 0A 05 5C 26 0C F7",
        "F0 7D 00 00 00 00 02 00 2F 00 4A 3F 30 0B 01 F7"
    )
    encoder(
        "comp/attack",
        "Comp: Attack",
        51,
        "F0 7D 00 00 00 00 02 00 37 7F 08 1E 44 7F 06 F7",
        "F0 7D 00 00 00 00 02 00 37 00 48 24 28 52 0B F7"
    )
    encoder(
        "drive",
        "Drive",
        15,
        "F0 7D 00 00 00 00 02 00 3C 7F 6B 6A 1D 34 09 F7",
        "F0 7D 00 00 00 00 02 00 3C 00 2B 50 71 19 04 F7"
    )
    -- Encoders in row 2
    encoder(
        "shape/gate",
        "Shape: Gate",
        54,
        "F0 7D 00 00 00 00 02 00 22 7F 3F 2B 22 6B 00 F7",
        "F0 7D 00 00 00 00 02 00 22 00 7F 11 4E 46 0D F7"
    )
    encoder(
        "comp/ratio",
        "Comp: Ratio",
        49,
        "F0 7D 00 00 00 00 02 00 35 7F 3A 28 26 7D 07 F7",
        "F0 7D 00 00 00 00 02 00 35 00 7A 12 4A 50 0A F7"
    )
    -- Encoders in row 3
    encoder(
        "low-cut",
        "Low Cut",
        103,
        "F0 7D 00 00 00 00 02 00 1D 7F 67 75 08 57 04 F7",
        "F0 7D 00 00 00 00 02 00 1D 00 27 4F 64 7A 09 F7"
    )
    encoder(
        "shape/sustain",
        "Shape: Sustain",
        55,
        "F0 7D 00 00 00 00 02 00 25 7F 70 6A 34 2C 03 F7",
        "F0 7D 00 00 00 00 02 00 25 00 30 50 58 01 0E F7"
    )
    encoder(
        "eq/low/freq",
        "EQ: Low Frequency",
        92,
        "F0 7D 00 00 00 00 02 00 29 7F 5C 5F 7B 20 0F F7",
        "F0 7D 00 00 00 00 02 00 29 00 1C 65 17 0D 02 F7"
    )
    encoder(
        "eq/low-mid/freq",
        "EQ: Low Mid Frequency",
        89,
        "F0 7D 00 00 00 00 02 00 2B 7F 6E 69 19 22 0E F7",
        "F0 7D 00 00 00 00 02 00 2B 00 2E 53 75 0F 03 F7"
    )
    encoder(
        "eq/high-mid/freq",
        "EQ: High Mid Frequency",
        86,
        "F0 7D 00 00 00 00 02 00 2E 7F 13 1E 6D 67 0C F7",
        "F0 7D 00 00 00 00 02 00 2E 00 53 24 01 4A 01 F7"
    )
    encoder(
        "eq/high/freq",
        "EQ: High Frequency",
        83,
        "F0 7D 00 00 00 00 02 00 32 7F 75 69 30 3A 04 F7",
        "F0 7D 00 00 00 00 02 00 32 00 35 53 5C 17 09 F7"
    )
    encoder(
        "comp/release",
        "Comp: Release",
        48,
        "F0 7D 00 00 00 00 02 00 38 7F 0F 06 58 30 0B F7",
        "F0 7D 00 00 00 00 02 00 38 00 4F 3C 34 1D 06 F7"
    )
    encoder(
        "character",
        "Character",
        18,
        "F0 7D 00 00 00 00 02 00 3D 7F 72 71 2C 75 09 F7",
        "F0 7D 00 00 00 00 02 00 3D 00 32 4B 40 58 04 F7"
    )
    -- Encoders in row 4
    encoder(
        "input/gain",
        "Input Gain",
        107,
        "F0 7D 00 00 00 00 02 00 1B 7F 31 2F 2F 51 07 F7",
        "F0 7D 00 00 00 00 02 00 1B 00 71 15 43 7C 0A F7"
    )
    encoder(
        "comp/parallel-dry-wet",
        "Comp: Parallel Dry/Wet",
        50,
        "F0 7D 00 00 00 00 02 00 36 7F 11 05 75 3E 06 F7",
        "F0 7D 00 00 00 00 02 00 36 00 51 3F 19 13 0B F7"
    )
    -- Encoders in row 5
    encoder(
        "shape/punch",
        "Shape: Punch",
        57,
        "F0 7D 00 00 00 00 02 00 26 7F 5B 47 67 6F 02 F7",
        "F0 7D 00 00 00 00 02 00 26 00 1B 7D 0B 42 0F F7"
    )
    encoder(
        "eq/low/gain",
        "EQ: Low Gain",
        91,
        "F0 7D 00 00 00 00 02 00 2A 7F 77 72 28 63 0E F7",
        "F0 7D 00 00 00 00 02 00 2A 00 37 48 44 4E 03 F7"
    )
    encoder(
        "eq/low-mid/gain",
        "EQ: Low Mid Gain",
        88,
        "F0 7D 00 00 00 00 02 00 2D 7F 38 33 3E 24 0D F7",
        "F0 7D 00 00 00 00 02 00 2D 00 78 09 52 09 00 F7"
    )
    encoder(
        "eq/high-mid/gain",
        "EQ: High Mid Gain",
        85,
        "F0 7D 00 00 00 00 02 00 30 7F 47 5F 52 38 05 F7",
        "F0 7D 00 00 00 00 02 00 30 00 07 65 3E 15 08 F7"
    )
    encoder(
        "eq/high/gain",
        "EQ: High Gain",
        82,
        "F0 7D 00 00 00 00 02 00 33 7F 6C 72 01 7B 04 F7",
        "F0 7D 00 00 00 00 02 00 33 00 2C 48 6D 56 09 F7"
    )
    encoder(
        "comp/threshold",
        "Comp: Threshold",
        47,
        "F0 7D 00 00 00 00 02 00 39 7F 16 1D 69 71 0B F7",
        "F0 7D 00 00 00 00 02 00 39 00 56 27 05 5C 06 F7"
    )
    encoder(
        "pan",
        "Pan",
        10,
        "F0 7D 00 00 00 00 02 00 3E 7F 59 5C 7F 36 08 F7",
        "F0 7D 00 00 00 00 02 00 3E 00 19 66 13 1B 05 F7"
    )
    encoder(
        "output/volume",
        "Volume",
        7,
        "F0 7D 00 00 00 00 02 00 41 7F 71 7B 1A 0F 04 F7",
        "F0 7D 00 00 00 00 02 00 41 00 31 41 76 22 09 F7"
    )
    -- Meters
    meter("input/meter/left", "Input Meter: Left channel", 110)
    meter("input/meter/right", "Input Meter: Right channel", 111)
    meter("output/meter/left", "Output Meter: Left channel", 112)
    meter("output/meter/right", "Output Meter: Right channel", 113)
    meter("shape/meter", "Shape Meter", 114)
    meter("comp/meter", "Compressor Meter", 115)

    return realearn.Compartment {
        mappings = mappings,
    }
end

return module
