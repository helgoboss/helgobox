--!strict

local midi_script = require("midi_script_source_runtime")

local module = {}

--- There's no way to put pads in a pulsing state with the true RGB sys-ex messages, so
--- we are back to an old-fashioned color palette. Whatever, close enough.
local color_palette: { midi_script.RgbColor } = {
    { r = 0x61, g = 0x61, b = 0x61 },
    { r = 0xb3, g = 0xb3, b = 0xb3 },
    { r = 0xdd, g = 0xdd, b = 0xdd },
    { r = 0xff, g = 0xff, b = 0xff },
    { r = 0xfc, g = 0xb3, b = 0xb3 },
    { r = 0xfa, g = 0x61, b = 0x61 },
    { r = 0xdd, g = 0x61, b = 0x61 },
    { r = 0xb3, g = 0x61, b = 0x61 },
    { r = 0xfe, g = 0xf3, b = 0xd5 },
    { r = 0xfb, g = 0xb3, b = 0x61 },
    { r = 0xdd, g = 0x8c, b = 0x61 },
    { r = 0xb3, g = 0x76, b = 0x61 },
    { r = 0xfc, g = 0xee, b = 0xa1 },
    { r = 0xfb, g = 0xff, b = 0x61 },
    { r = 0xdd, g = 0xdd, b = 0x61 },
    { r = 0xb3, g = 0xb3, b = 0x61 },
    { r = 0xdd, g = 0xff, b = 0xa1 },
    { r = 0xc2, g = 0xff, b = 0x61 },
    { r = 0xa1, g = 0xdd, b = 0x61 },
    { r = 0x81, g = 0xb3, b = 0x61 },
    { r = 0xc2, g = 0xff, b = 0xb3 },
    { r = 0x61, g = 0xfe, b = 0x61 },
    { r = 0x61, g = 0xdd, b = 0x61 },
    { r = 0x61, g = 0xb3, b = 0x61 },
    { r = 0xc2, g = 0xff, b = 0xc2 },
    { r = 0x61, g = 0xfe, b = 0x8c },
    { r = 0x61, g = 0xdd, b = 0x76 },
    { r = 0x61, g = 0xb3, b = 0x6b },
    { r = 0xc2, g = 0xfe, b = 0xcc },
    { r = 0x61, g = 0xfe, b = 0xcc },
    { r = 0x61, g = 0xdd, b = 0xa1 },
    { r = 0x61, g = 0xb3, b = 0x81 },
    { r = 0xc2, g = 0xfe, b = 0xf3 },
    { r = 0x61, g = 0xfd, b = 0xe9 },
    { r = 0x61, g = 0xdd, b = 0xc2 },
    { r = 0x61, g = 0xb3, b = 0x96 },
    { r = 0xc2, g = 0xf3, b = 0xff },
    { r = 0x61, g = 0xee, b = 0xff },
    { r = 0x61, g = 0xc7, b = 0xdd },
    { r = 0x61, g = 0xa1, b = 0xb3 },
    { r = 0xc2, g = 0xdd, b = 0xff },
    { r = 0x61, g = 0xc7, b = 0xff },
    { r = 0x61, g = 0xa1, b = 0xdd },
    { r = 0x61, g = 0x81, b = 0xb3 },
    { r = 0xa1, g = 0x8c, b = 0xff },
    { r = 0x61, g = 0x61, b = 0xff },
    { r = 0x61, g = 0x61, b = 0xdd },
    { r = 0x61, g = 0x61, b = 0xb3 },
    { r = 0xcc, g = 0xb3, b = 0xff },
    { r = 0xa1, g = 0x61, b = 0xff },
    { r = 0x81, g = 0x61, b = 0xdd },
    { r = 0x76, g = 0x61, b = 0xb3 },
    { r = 0xfe, g = 0xb3, b = 0xff },
    { r = 0xfe, g = 0x61, b = 0xff },
    { r = 0xdd, g = 0x61, b = 0xdd },
    { r = 0xb3, g = 0x61, b = 0xb3 },
    { r = 0xfd, g = 0xb3, b = 0xd5 },
    { r = 0xfc, g = 0x61, b = 0xc2 },
    { r = 0xdd, g = 0x61, b = 0xa1 },
    { r = 0xb3, g = 0x61, b = 0x8c },
    { r = 0xfa, g = 0x76, b = 0x61 },
    { r = 0xe9, g = 0xb3, b = 0x61 },
    { r = 0xdd, g = 0xc2, b = 0x61 },
    { r = 0xa1, g = 0xa1, b = 0x61 },
    { r = 0x61, g = 0xb3, b = 0x61 },
    { r = 0x61, g = 0xb3, b = 0x8c },
    { r = 0x61, g = 0x8c, b = 0xd5 },
    { r = 0x61, g = 0x61, b = 0xff },
    { r = 0x61, g = 0xb3, b = 0xb3 },
    { r = 0x8c, g = 0x62, b = 0xf3 },
    { r = 0xcc, g = 0xb3, b = 0xc2 },
    { r = 0x8c, g = 0x76, b = 0x81 },
    { r = 0xfa, g = 0x61, b = 0x61 },
    { r = 0xf3, g = 0xff, b = 0xa1 },
    { r = 0xee, g = 0xfc, b = 0x61 },
    { r = 0xcc, g = 0xff, b = 0x60 },
    { r = 0x76, g = 0xdd, b = 0x61 },
    { r = 0x61, g = 0xfe, b = 0xcc },
    { r = 0x61, g = 0xe9, b = 0xff },
    { r = 0x61, g = 0xa1, b = 0xff },
    { r = 0x8c, g = 0x61, b = 0xff },
    { r = 0xcc, g = 0x61, b = 0xfc },
    { r = 0xee, g = 0x8c, b = 0xdd },
    { r = 0xa1, g = 0x76, b = 0x61 },
    { r = 0xfb, g = 0xa1, b = 0x61 },
    { r = 0xdd, g = 0xf9, b = 0x62 },
    { r = 0xd5, g = 0xff, b = 0x8c },
    { r = 0x61, g = 0xfe, b = 0x61 },
    { r = 0xb3, g = 0xff, b = 0xa1 },
    { r = 0xcc, g = 0xfc, b = 0xd5 },
    { r = 0xb3, g = 0xfe, b = 0xf6 },
    { r = 0xcc, g = 0xe4, b = 0xff },
    { r = 0xa1, g = 0xc2, b = 0xf6 },
    { r = 0xd5, g = 0xc2, b = 0xf9 },
    { r = 0xf9, g = 0x8c, b = 0xff },
    { r = 0xfc, g = 0x61, b = 0xcc },
    { r = 0xfb, g = 0xc2, b = 0x61 },
    { r = 0xf3, g = 0xee, b = 0x61 },
    { r = 0xe4, g = 0xff, b = 0x61 },
    { r = 0xdd, g = 0xcc, b = 0x61 },
    { r = 0xb3, g = 0xa1, b = 0x61 },
    { r = 0x61, g = 0xba, b = 0x76 },
    { r = 0x76, g = 0xc2, b = 0x8c },
    { r = 0x81, g = 0x81, b = 0xa1 },
    { r = 0x81, g = 0x8c, b = 0xcc },
    { r = 0xcc, g = 0xaa, b = 0x81 },
    { r = 0xdd, g = 0x61, b = 0x61 },
    { r = 0xf9, g = 0xb3, b = 0xa1 },
    { r = 0xf9, g = 0xba, b = 0x76 },
    { r = 0xfc, g = 0xf3, b = 0x8d },
    { r = 0xe9, g = 0xf9, b = 0xa1 },
    { r = 0xd5, g = 0xee, b = 0x76 },
    { r = 0x81, g = 0x81, b = 0xa1 },
    { r = 0xf9, g = 0xf9, b = 0xd5 },
    { r = 0xdd, g = 0xfc, b = 0xe4 },
    { r = 0xe9, g = 0xe9, b = 0xff },
    { r = 0xe4, g = 0xd5, b = 0xff },
    { r = 0xb3, g = 0xb3, b = 0xb3 },
    { r = 0xd5, g = 0xd5, b = 0xd5 },
    { r = 0xf9, g = 0xff, b = 0xff },
    { r = 0xe9, g = 0x61, b = 0x61 },
    { r = 0xaa, g = 0x62, b = 0x61 },
    { r = 0x81, g = 0xf6, b = 0x62 },
    { r = 0x61, g = 0xb3, b = 0x61 },
    { r = 0xf3, g = 0xee, b = 0x61 },
    { r = 0xb3, g = 0xa1, b = 0x61 },
    { r = 0xee, g = 0xc2, b = 0x61 },
    { r = 0xc2, g = 0x76, b = 0x61 },
}

--- Returns the 0-based index within the given palette array.
local function find_closest_color_in_palette(color: midi_script.RgbColor, palette: { midi_script.RgbColor }): number
    local ifurthest = 0
    local furthest = 3 * math.pow(255, 2) + 1
    for i, c in palette do
        if color.r == c.r and color.g == c.g and color.b == c.b then
            return i - 1
        end
        local distance = math.pow((color.r - c.r), 2) + math.pow((color.g - c.g), 2) + math.pow((color.b - c.b), 2)
        if distance < furthest then
            furthest = distance
            ifurthest = i - 1
        end
    end
    return ifurthest
end

type FeedbackEntry = {
    behavior: number,
    color: number?,
}
local solid = 0x90
local flashing = 0x91
local pulsing = 0x92
local black_color = 0
local white_color = 3
local red_color = 5

local feedback_table: { [string]: FeedbackEntry? } = {
    ["playtime.slot_state.empty"] = {
        behavior = solid,
        color = black_color,
    },
    ["playtime.slot_state.armed"] = {
        behavior = solid,
        color = red_color,
    },
    ["playtime.slot_state.stopped"] = {
        behavior = solid,
    },
    ["playtime.slot_state.scheduled_for_play_start"] = {
        behavior = flashing,
    },
    ["playtime.slot_state.playing"] = {
        behavior = pulsing,
    },
    ["playtime.slot_state.paused"] = {
        behavior = solid,
    },
    ["playtime.slot_state.scheduled_for_play_stop"] = {
        behavior = flashing,
    },
    ["playtime.slot_state.scheduled_for_record_start"] = {
        behavior = flashing,
        color = red_color,
    },
    ["playtime.slot_state.recording"] = {
        behavior = pulsing,
        color = red_color,
    },
    ["playtime.slot_state.scheduled_for_record_stop"] = {
        behavior = flashing,
        color = red_color,
    },
}

local function build_messages(
    pad_index: number,
    behavior: number,
    color_index: number
): { midi_script.MidiMessage }
    if behavior == pulsing then
        return {
            { 0x92, pad_index, color_index },
        }
    elseif behavior == flashing then
        -- Flashing between actual color and black
        return {
            { 0x90, pad_index, color_index },
            { 0x91, pad_index, black_color },
        }
    else
        -- Solid
        return {
            { 0x90, pad_index, color_index },
        }
    end
end

local function get_palette_color_index(entry: FeedbackEntry, rgb_color: midi_script.RgbColor?): number 
    local color_index = entry.color
    if color_index then
        return color_index
    end
    if rgb_color == nil then
        return white_color
    end
    return find_closest_color_in_palette(rgb_color, color_palette)
end

function module.pad_script(
    pad_index: number,
    y: midi_script.InputValue,
    context: midi_script.Context
): midi_script.Output
    local address = pad_index
    local off_output = {
        address = pad_index,
        messages = build_messages(pad_index, solid, black_color),
    }
    -- Handle "off" feedback
    if y == nil then
        return off_output
    end
    -- Handle numeric feedback
    if type(y) == "number" then
        -- Translate number to solid feedback color
        local color_index = math.floor(y * 127)
        return {
            address = address,
            messages = {
                { solid, pad_index, color_index },
            },
        }
    end
    -- Handle text feedback
    if type(y) == "string" then
        -- This is a grid controller, typically used for controlling a clip matrix, so we should be able to
        -- handle text feedback that conforms to Playtime's clip state convention.
        local entry = feedback_table[y]
        if entry == nil then
            -- Unknown texts switch the LED off
            return off_output
        end
        local color_index = get_palette_color_index(entry, context.feedback_event.color)
        return {
            address = pad_index,
            messages = build_messages(pad_index, entry.behavior, color_index),
        }
    end
    -- Complex feedback is highly individual. It doesn't make sense to handle this in a general-purpose controller preset.
    return {
        address = address,
        messages = {},
    }
end

return module
