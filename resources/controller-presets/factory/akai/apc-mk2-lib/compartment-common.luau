--!strict

local midi_script = require("midi_script_source_runtime")

local module = {}

type FeedbackEntry = {
    behavior: number,
    color: number?,
}
local dark = 0x91
local medium = 0x93
local bright = 0x96
local pulsing = 0x9A
local blinking = 0x9B
local red_color = 6
local default_color = 3
local feedback_table: { [string]: FeedbackEntry? } = {
    ["playtime.slot_state.empty"] = {
        behavior = dark,
        color = 0,
    },
    ["playtime.slot_state.armed"] = {
        behavior = dark,
        color = red_color,
    },
    ["playtime.slot_state.stopped"] = {
        behavior = dark,
    },
    ["playtime.slot_state.ignited"] = {
        behavior = dark,
    },
    ["playtime.slot_state.scheduled_for_play_start"] = {
        behavior = medium,
    },
    ["playtime.slot_state.playing"] = {
        behavior = pulsing,
    },
    ["playtime.slot_state.paused"] = {
        behavior = medium,
    },
    ["playtime.slot_state.scheduled_for_play_stop"] = {
        behavior = blinking,
    },
    ["playtime.slot_state.scheduled_for_play_restart"] = {
        behavior = blinking,
    },
    ["playtime.slot_state.scheduled_for_record_start"] = {
        behavior = medium,
        color = red_color,
    },
    ["playtime.slot_state.recording"] = {
        behavior = pulsing,
        color = red_color,
    },
    ["playtime.slot_state.scheduled_for_record_stop"] = {
        behavior = blinking,
        color = red_color,
    },
}

--- Although I got the true RGB colors to work, it was impossible to combine them with blinking
--- or pulsing behavior. So back to good old color palette.
local color_palette: { midi_script.RgbColor } = {
    {r = 0x00, g= 0x00, b = 0x00},
    {r = 0x1E, g= 0x1E, b = 0x1E},
    {r = 0x7F, g= 0x7F, b = 0x7F},
    {r = 0xFF, g= 0xFF, b = 0xFF},
    {r = 0xFF, g= 0x4C, b = 0x4C},
    {r = 0xFF, g= 0x00, b = 0x00},
    {r = 0x59, g= 0x00, b = 0x00},
    {r = 0x19, g= 0x00, b = 0x00},
    {r = 0xFF, g= 0xBD, b = 0x6C},
    {r = 0xFF, g= 0x54, b = 0x00},
    {r = 0x59, g= 0x1D, b = 0x00},
    {r = 0x27, g= 0x1B, b = 0x00},
    {r = 0xFF, g= 0xFF, b = 0x4C},
    {r = 0xFF, g= 0xFF, b = 0x00},
    {r = 0x59, g= 0x59, b = 0x00},
    {r = 0x19, g= 0x19, b = 0x00},
    {r = 0x88, g= 0xFF, b = 0x4C},
    {r = 0x54, g= 0xFF, b = 0x00},
    {r = 0x1D, g= 0x59, b = 0x00},
    {r = 0x14, g= 0x2B, b = 0x00},
    {r = 0x4C, g= 0xFF, b = 0x4C},
    {r = 0x00, g= 0xFF, b = 0x00},
    {r = 0x00, g= 0x59, b = 0x00},
    {r = 0x00, g= 0x19, b = 0x00},
    {r = 0x4C, g= 0xFF, b = 0x5E},
    {r = 0x00, g= 0xFF, b = 0x19},
    {r = 0x00, g= 0x59, b = 0x0D},
    {r = 0x00, g= 0x19, b = 0x02},
    {r = 0x4C, g= 0xFF, b = 0x88},
    {r = 0x00, g= 0xFF, b = 0x55},
    {r = 0x00, g= 0x59, b = 0x1D},
    {r = 0x00, g= 0x1F, b = 0x12},
    {r = 0x4C, g= 0xFF, b = 0xB7},
    {r = 0x00, g= 0xFF, b = 0x99},
    {r = 0x00, g= 0x59, b = 0x35},
    {r = 0x00, g= 0x19, b = 0x12},
    {r = 0x4C, g= 0xC3, b = 0xFF},
    {r = 0x00, g= 0xA9, b = 0xFF},
    {r = 0x00, g= 0x41, b = 0x52},
    {r = 0x00, g= 0x10, b = 0x19},
    {r = 0x4C, g= 0x88, b = 0xFF},
    {r = 0x00, g= 0x55, b = 0xFF},
    {r = 0x00, g= 0x1D, b = 0x59},
    {r = 0x00, g= 0x08, b = 0x19},
    {r = 0x4C, g= 0x4C, b = 0xFF},
    {r = 0x00, g= 0x00, b = 0xFF},
    {r = 0x00, g= 0x00, b = 0x59},
    {r = 0x00, g= 0x00, b = 0x19},
    {r = 0x87, g= 0x4C, b = 0xFF},
    {r = 0x54, g= 0x00, b = 0xFF},
    {r = 0x19, g= 0x00, b = 0x64},
    {r = 0x0F, g= 0x00, b = 0x30},
    {r = 0xFF, g= 0x4C, b = 0xFF},
    {r = 0xFF, g= 0x00, b = 0xFF},
    {r = 0x59, g= 0x00, b = 0x59},
    {r = 0x19, g= 0x00, b = 0x19},
    {r = 0xFF, g= 0x4C, b = 0x87},
    {r = 0xFF, g= 0x00, b = 0x54},
    {r = 0x59, g= 0x00, b = 0x1D},
    {r = 0x22, g= 0x00, b = 0x13},
    {r = 0xFF, g= 0x15, b = 0x00},
    {r = 0x99, g= 0x35, b = 0x00},
    {r = 0x79, g= 0x51, b = 0x00},
    {r = 0x43, g= 0x64, b = 0x00},
    {r = 0x03, g= 0x39, b = 0x00},
    {r = 0x00, g= 0x57, b = 0x35},
    {r = 0x00, g= 0x54, b = 0x7F},
    {r = 0x00, g= 0x00, b = 0xFF},
    {r = 0x00, g= 0x45, b = 0x4F},
    {r = 0x25, g= 0x00, b = 0xCC},
    {r = 0x7F, g= 0x7F, b = 0x7F},
    {r = 0x20, g= 0x20, b = 0x20},
    {r = 0xFF, g= 0x00, b = 0x00},
    {r = 0xBD, g= 0xFF, b = 0x2D},
    {r = 0xAF, g= 0xED, b = 0x06},
    {r = 0x64, g= 0xFF, b = 0x09},
    {r = 0x10, g= 0x8B, b = 0x00},
    {r = 0x00, g= 0xFF, b = 0x87},
    {r = 0x00, g= 0xA9, b = 0xFF},
    {r = 0x00, g= 0x2A, b = 0xFF},
    {r = 0x3F, g= 0x00, b = 0xFF},
    {r = 0x7A, g= 0x00, b = 0xFF},
    {r = 0xB2, g= 0x1A, b = 0x7D},
    {r = 0x40, g= 0x21, b = 0x00},
    {r = 0xFF, g= 0x4A, b = 0x00},
    {r = 0x88, g= 0xE1, b = 0x06},
    {r = 0x72, g= 0xFF, b = 0x15},
    {r = 0x00, g= 0xFF, b = 0x00},
    {r = 0x3B, g= 0xFF, b = 0x26},
    {r = 0x59, g= 0xFF, b = 0x71},
    {r = 0x38, g= 0xFF, b = 0xCC},
    {r = 0x5B, g= 0x8A, b = 0xFF},
    {r = 0x31, g= 0x51, b = 0xC6},
    {r = 0x87, g= 0x7F, b = 0xE9},
    {r = 0xD3, g= 0x1D, b = 0xFF},
    {r = 0xFF, g= 0x00, b = 0x5D},
    {r = 0xFF, g= 0x7F, b = 0x00},
    {r = 0xB9, g= 0xB0, b = 0x00},
    {r = 0x90, g= 0xFF, b = 0x00},
    {r = 0x83, g= 0x5D, b = 0x07},
    {r = 0x39, g= 0x2b, b = 0x00},
    {r = 0x14, g= 0x4C, b = 0x10},
    {r = 0x0D, g= 0x50, b = 0x38},
    {r = 0x15, g= 0x15, b = 0x2A},
    {r = 0x16, g= 0x20, b = 0x5A},
    {r = 0x69, g= 0x3C, b = 0x1C},
    {r = 0xA8, g= 0x00, b = 0x0A},
    {r = 0xDE, g= 0x51, b = 0x3D},
    {r = 0xD8, g= 0x6A, b = 0x1C},
    {r = 0xFF, g= 0xE1, b = 0x26},
    {r = 0x9E, g= 0xE1, b = 0x2F},
    {r = 0x67, g= 0xB5, b = 0x0F},
    {r = 0x1E, g= 0x1E, b = 0x30},
    {r = 0xDC, g= 0xFF, b = 0x6B},
    {r = 0x80, g= 0xFF, b = 0xBD},
    {r = 0x9A, g= 0x99, b = 0xFF},
    {r = 0x8E, g= 0x66, b = 0xFF},
    {r = 0x40, g= 0x40, b = 0x40},
    {r = 0x75, g= 0x75, b = 0x75},
    {r = 0xE0, g= 0xFF, b = 0xFF},
    {r = 0xA0, g= 0x00, b = 0x00},
    {r = 0x35, g= 0x00, b = 0x00},
    {r = 0x1A, g= 0xD0, b = 0x00},
    {r = 0x07, g= 0x42, b = 0x00},
    {r = 0xB9, g= 0xB0, b = 0x00},
    {r = 0x3F, g= 0x31, b = 0x00},
    {r = 0xB3, g= 0x5F, b = 0x00},
    {r = 0x4B, g= 0x15, b = 0x02},
    {r = 0x40, g= 0x40, b = 0x40},
    {r = 0x75, g= 0x75, b = 0x75},
    {r = 0xE0, g= 0xFF, b = 0xFF},
    {r = 0xA0, g= 0x00, b = 0x00},
    {r = 0x35, g= 0x00, b = 0x00},
    {r = 0x1A, g= 0xD0, b = 0x00},
    {r = 0x07, g= 0x42, b = 0x00},
    {r = 0xB9, g= 0xB0, b = 0x00},
    {r = 0x3F, g= 0x31, b = 0x00},
    {r = 0xB3, g= 0x5F, b = 0x00},
    {r = 0x4B, g= 0x15, b = 0x02},
}

--- Returns the 0-based index within the given palette array.
local function find_closest_color_in_palette(color: midi_script.RgbColor, palette: {midi_script.RgbColor}): number
    local ifurthest = 0
    local furthest = 3 * math.pow(255, 2) + 1
    for i,c in palette do
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

function module.pad_script(pad_index: number, y: midi_script.InputValue, context: midi_script.Context): midi_script.Output
    local address = pad_index
    local off_color = 0x00
    local off_output = {
        address = address,
        messages = {
            { bright, pad_index, off_color },
        },
    }
    -- Handle "off" feedback
    if y == nil then
        return off_output
    end
    -- Handle numeric feedback
    if type(y) == "number" then
        -- Translate number to solid feedback color, just as with APC Key 25 mk1
        local color = math.floor(y * 127)
        return {
            address = address,
            messages = {
                { bright, pad_index, color },
            },
        }
    end
    -- Handle text feedback
    if type(y) == "string" then
        -- This is a grid controller, typically used for controlling a Playtime matrix, so we should be able to
        -- handle text feedback that conforms to Playtime's clip state convention.
        local entry = feedback_table[y]
        if entry == nil then
            -- Unknown texts switch the LED off
            return off_output
        end
        local explicit_color = entry.color
        if explicit_color ~= nil then
            -- Switch LED to explicit color
            return {
                address = address,
                messages = {
                    { entry.behavior, pad_index, explicit_color },
                },
            }
        end
        -- No explicit color means we should use the event color (which is set in the "Glue" section)
        local event_color = context.feedback_event.color
        if event_color == nil then
            -- Event has default color / no specific color
            return {
                address = address,
                messages = {
                    { entry.behavior, pad_index, default_color },
                },
            }
        end
        -- Find color available on the controller that's closest to the desired RGB color
        local closest_color = find_closest_color_in_palette(event_color, color_palette)
        return {
            address = address,
            messages = {
                { entry.behavior, pad_index, closest_color },
            },
        }
    end
    -- Complex feedback is highly individual. It doesn't make sense to handle this in a general-purpose controller preset.
    return {
        address = address,
        messages = {},
    }
end

return module