--!strict

local midi_script = require("midi_script_source_runtime")

local module = {}

local color_palette: { midi_script.RgbColor } = {
    { r = 0, g = 0, b = 0 },
    { r = 0, g = 0, b = 255 },
    { r = 0, g = 21, b = 255 },
    { r = 0, g = 34, b = 255 },
    { r = 0, g = 46, b = 255 },
    { r = 0, g = 59, b = 255 },
    { r = 0, g = 68, b = 255 },
    { r = 0, g = 80, b = 255 },
    { r = 0, g = 93, b = 255 },
    { r = 0, g = 106, b = 255 },
    { r = 0, g = 119, b = 255 },
    { r = 0, g = 127, b = 255 },
    { r = 0, g = 140, b = 255 },
    { r = 0, g = 153, b = 255 },
    { r = 0, g = 165, b = 255 },
    { r = 0, g = 178, b = 255 },
    { r = 0, g = 191, b = 255 },
    { r = 0, g = 199, b = 255 },
    { r = 0, g = 212, b = 255 },
    { r = 0, g = 225, b = 255 },
    { r = 0, g = 238, b = 255 },
    { r = 0, g = 250, b = 255 },
    { r = 0, g = 255, b = 250 },
    { r = 0, g = 255, b = 237 },
    { r = 0, g = 255, b = 225 },
    { r = 0, g = 255, b = 212 },
    { r = 0, g = 255, b = 199 },
    { r = 0, g = 255, b = 191 },
    { r = 0, g = 255, b = 178 },
    { r = 0, g = 255, b = 165 },
    { r = 0, g = 255, b = 153 },
    { r = 0, g = 255, b = 140 },
    { r = 0, g = 255, b = 127 },
    { r = 0, g = 255, b = 119 },
    { r = 0, g = 255, b = 106 },
    { r = 0, g = 255, b = 93 },
    { r = 0, g = 255, b = 80 },
    { r = 0, g = 255, b = 67 },
    { r = 0, g = 255, b = 59 },
    { r = 0, g = 255, b = 46 },
    { r = 0, g = 255, b = 33 },
    { r = 0, g = 255, b = 21 },
    { r = 0, g = 255, b = 8 },
    { r = 0, g = 255, b = 0 },
    { r = 12, g = 255, b = 0 },
    { r = 25, g = 255, b = 0 },
    { r = 38, g = 255, b = 0 },
    { r = 51, g = 255, b = 0 },
    { r = 63, g = 255, b = 0 },
    { r = 72, g = 255, b = 0 },
    { r = 84, g = 255, b = 0 },
    { r = 97, g = 255, b = 0 },
    { r = 110, g = 255, b = 0 },
    { r = 123, g = 255, b = 0 },
    { r = 131, g = 255, b = 0 },
    { r = 144, g = 255, b = 0 },
    { r = 157, g = 255, b = 0 },
    { r = 170, g = 255, b = 0 },
    { r = 182, g = 255, b = 0 },
    { r = 191, g = 255, b = 0 },
    { r = 203, g = 255, b = 0 },
    { r = 216, g = 255, b = 0 },
    { r = 229, g = 255, b = 0 },
    { r = 242, g = 255, b = 0 },
    { r = 255, g = 255, b = 0 },
    { r = 255, g = 246, b = 0 },
    { r = 255, g = 233, b = 0 },
    { r = 255, g = 220, b = 0 },
    { r = 255, g = 208, b = 0 },
    { r = 255, g = 195, b = 0 },
    { r = 255, g = 187, b = 0 },
    { r = 255, g = 174, b = 0 },
    { r = 255, g = 161, b = 0 },
    { r = 255, g = 148, b = 0 },
    { r = 255, g = 135, b = 0 },
    { r = 255, g = 127, b = 0 },
    { r = 255, g = 114, b = 0 },
    { r = 255, g = 102, b = 0 },
    { r = 255, g = 89, b = 0 },
    { r = 255, g = 76, b = 0 },
    { r = 255, g = 63, b = 0 },
    { r = 255, g = 55, b = 0 },
    { r = 255, g = 42, b = 0 },
    { r = 255, g = 29, b = 0 },
    { r = 255, g = 16, b = 0 },
    { r = 255, g = 4, b = 0 },
    { r = 255, g = 0, b = 4 },
    { r = 255, g = 0, b = 16 },
    { r = 255, g = 0, b = 29 },
    { r = 255, g = 0, b = 42 },
    { r = 255, g = 0, b = 55 },
    { r = 255, g = 0, b = 63 },
    { r = 255, g = 0, b = 76 },
    { r = 255, g = 0, b = 89 },
    { r = 255, g = 0, b = 102 },
    { r = 255, g = 0, b = 114 },
    { r = 255, g = 0, b = 127 },
    { r = 255, g = 0, b = 135 },
    { r = 255, g = 0, b = 148 },
    { r = 255, g = 0, b = 161 },
    { r = 255, g = 0, b = 174 },
    { r = 255, g = 0, b = 186 },
    { r = 255, g = 0, b = 195 },
    { r = 255, g = 0, b = 208 },
    { r = 255, g = 0, b = 221 },
    { r = 255, g = 0, b = 233 },
    { r = 255, g = 0, b = 246 },
    { r = 255, g = 0, b = 255 },
    { r = 242, g = 0, b = 255 },
    { r = 229, g = 0, b = 255 },
    { r = 216, g = 0, b = 255 },
    { r = 204, g = 0, b = 255 },
    { r = 191, g = 0, b = 255 },
    { r = 182, g = 0, b = 255 },
    { r = 169, g = 0, b = 255 },
    { r = 157, g = 0, b = 255 },
    { r = 144, g = 0, b = 255 },
    { r = 131, g = 0, b = 255 },
    { r = 123, g = 0, b = 255 },
    { r = 110, g = 0, b = 255 },
    { r = 97, g = 0, b = 255 },
    { r = 85, g = 0, b = 255 },
    { r = 72, g = 0, b = 255 },
    { r = 63, g = 0, b = 255 },
    { r = 50, g = 0, b = 255 },
    { r = 38, g = 0, b = 255 },
    { r = 25, g = 0, b = 255 },
    { r = 240, g = 240, b = 225 },
}

type FeedbackEntry = {
    behavior: number,
    color: number?,
}
local dark = 30
local medium = 40
local bright = 47
local pulsing = 12
local blinking = 8
local red_color = 83
local default_color = 21
local feedback_table: { [string]: FeedbackEntry? } = {
    ["playtime.slot_state.empty"] = {
        behavior = bright,
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
        behavior = dark,
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

local function create_output(pad_index: number, color_index: number, behavior: number): midi_script.Output
    return {
        address = pad_index,
        messages = {
            { 0xB1, pad_index, color_index },
            { 0xB2, pad_index, behavior },
        },
    }
end

function module.pad_script(
    pad_index: number,
    y: midi_script.InputValue,
    context: midi_script.Context
): midi_script.Output
    local off_color = 0x00
    local off_output = create_output(pad_index, off_color, bright)
    -- Handle "off" feedback
    if y == nil then
        return off_output
    end
    -- Handle numeric feedback
    if type(y) == "number" then
        -- Translate number to solid feedback color.
        local color = math.floor(y * 127)
        return create_output(pad_index, color, bright)
    end
    -- Handle text feedback
    if type(y) == "string" then
        -- This can be used as a grid controller for controlling a clip matrix, so we should be able to
        -- handle text feedback that conforms to Playtime's clip state convention.
        local entry = feedback_table[y]
        if entry == nil then
            -- Unknown texts switch the LED off
            return off_output
        end
        local explicit_color = entry.color
        if explicit_color ~= nil then
            -- Switch LED to explicit color
            return create_output(pad_index, explicit_color, entry.behavior)
        end
        -- No explicit color means we should use the event color (which is set in the "Glue" section)
        local event_color = context.feedback_event.color
        if event_color == nil then
            -- Event has default color / no specific color
            return create_output(pad_index, default_color, entry.behavior)
        end
        -- Find color available on the controller that's closest to the desired RGB color
        local closest_color = find_closest_color_in_palette(event_color, color_palette)
        return create_output(pad_index, closest_color, entry.behavior)
    end
    -- Complex feedback is highly individual. It doesn't make sense to handle this in a general-purpose controller preset.
    return {
        address = pad_index,
        messages = {},
    }
end

return module
