local reusable_lua_code = [[
-- ## Constants ##

local black = { r = 0, g = 0, b = 0 }
local white = { r = 255, g = 255, b = 255 }

-- ## Functions ##

function to_ascii(text)
    local bytes = {}
    if text ~= nil then
        for i = 1, string.len(text) do
            bytes[i] = string.byte(text, i)
        end
    end
    -- Null terminator
    table.insert(bytes, 0x00)
    return bytes
end

function concat_table(t1, t2)
    for i = 1, #t2 do
        t1[#t1 + 1] = t2[i]
    end
end

function create_msg(content)
    -- SysEx Header
    local msg = {
        0xf0, 0x00, 0x20, 0x29, 0x02, 0x0a, 0x01,
    }
    -- Content
    concat_table(msg, content)
    -- End of SysEx
    table.insert(msg, 0xf7)
    return msg
end

--- Creates a message for changing the layout.
---
--- @param layout_index number layout index (0 = empty, 1 = knob, 2 = box)
function create_screen_layout_msg(layout_index)
    return create_msg({ 0x01, layout_index })
end

--- Creates a message for displaying a notification on the center screen.
---
--- @param line1 string first line of text
--- @param line2 string second line of text
function create_notification_text_msg(line1, line2)
    local content = { 0x04 }
    concat_table(content, to_ascii(line1))
    concat_table(content, to_ascii(line2))
    return create_msg(content)
end

--- Creates a message for changing multiple screen properties.
---
--- @param changes table list of property changes
function create_screen_props_msg(changes)
    local content = { 0x02 }
    for i = 1, #changes do
        if changes[i] ~= nil then
            concat_table(content, changes[i])
        end
    end
    return create_msg(content)
end

--- Creates a text property change.
---
--- @param column_index number in which column to display the text
--- @param object_index number in which location to display the text
--- @param text string the actual text
function create_text_prop_change(column_index, object_index, text)
    local change = {
        -- Column Index
        column_index,
        -- Property Type "Text"
        0x01,
        -- Object Index
        object_index,
    }
    concat_table(change, to_ascii(text))
    return change
end

--- Creates a value property change.
---
--- If it's about setting the knob value, it's usually more intuitive to not
--- do this via script but by enabling feedback on the corresponding encoder
--- mappings, then you need just one mapping for both control and feedback.
---
--- @param column_index number in which column to change the value
--- @param object_index number the kind of value to change (see programmer's guide)
--- @param value number the value
function create_value_prop_change(column_index, object_index, value)
    return {
        -- Column Index
        column_index,
        -- Property Type "Value"
        0x03,
        -- Object Index
        object_index,
        -- Color bytes
        value
    }
end

--- Creates an RGB color property change.
---
--- If the given color is `nil`, this function returns `nil`.
---
--- @param column_index number in which column to change the color
--- @param object_index number in which location to change the color
--- @param color table the RGB color (table with properties r, g and b)
function create_rgb_color_prop_change(column_index, object_index, color)
    if color == null then
        return nil
    end
    return {
        -- Column Index
        column_index,
        -- Property Type "RGB color"
        0x04,
        -- Object Index
        object_index,
        -- Color bytes
        math.floor(color.r / 2),
        math.floor(color.g / 2),
        math.floor(color.b / 2),
    }
end

-- ## Code ##
]]
local mode_count = 10
local parameters = {
    {
        index = 0,
        name = "Mode",
        value_count = mode_count,
    },
    {
        index = 1,
        name = "Macro bank",
        value_count = 100,
    },
}
local browse_mode_condition ={
    kind = "Bank",
    parameter = 0,
    bank_index = 0,
}
local macro_mode_condition ={
    kind = "Bank",
    parameter = 0,
    bank_index = 1,
}
local groups = {
    {
        id = "modes",
        name = "Modes",
    },
    {
        id = "macro-banks",
        name = "Macro banks",
        activation_condition = macro_mode_condition,
    },
    {
        id = "macro-parameters",
        name = "Macro parameters",
        activation_condition = macro_mode_condition,
    },
    {
        id = "macro-resolved-parameters",
        name = "Macro resolved parameters",
        activation_condition = macro_mode_condition,
    },
    {
        id = "init",
        name = "Initialization",
    },
    {
        id = "manual",
        name = "Manual",
    },
}
local turbo_mode = {
    kind = "AfterTimeoutKeepFiring",
    timeout = 0,
    rate = 150,
}
local mappings = {
    {
        name = "Mode switch",
        group = "modes",
        source = {
            kind = "MidiControlChangeValue",
            channel = 15,
            controller_number = 90,
            character = "Button",
            fourteen_bit = false,
        },
        glue = {
            absolute_mode = "IncrementalButton",
            source_interval = {0.16, 1},
            target_interval = {0, 0.1111111111111111},
            wrap = true,
            step_size_interval = {0.1111111111111111, 0.1111111111111111},
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 0,
            },
        },
    },
    {
        name = "Init screens",
        group = "init",
        source = {
            kind = "MidiScript",
            script_kind = "lua",
            script = reusable_lua_code .. [[

local mode = y
local layout
local text
if mode == nil then
    layout = 0
    text = ""
elseif mode == 0 then
    layout = 2
    text = "Browse"
elseif mode == 1 then
    layout = 1
    text = "Macros"
end

return {
    messages = {
        create_notification_text_msg("Initializing", "Pot Control"),
        create_screen_layout_msg(layout),
        create_screen_props_msg({
            create_text_prop_change(8, 0, text),
        }),
    }
} ]],
        },
        glue = {
            target_interval = {0, 0.1111111111111111},
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 0,
            },
        },
    },
    {
        name = "Set instance FX",
        group = "manual",
        feedback_enabled = false,
        glue = {
            step_size_interval = { 0.01, 0.05 },
            step_factor_interval = { 1, 5 },
        },
        target = {
            kind = "Fx",
            fx = {
                address = "ByIndex",
                chain = {
                    address = "Track",
                    track = {
                        address = "ById",
                        id = "8C6F9DDC-A5E4-DC49-80E7-C79A373FA51B",
                    },
                },
                index = 0,
            },
            action = "SetAsInstanceFx",
        },
    },
    {
        name = "Up - Previous macro bank",
        group = "macro-banks",
        feedback_enabled = false,
        source = {
            kind = "MidiControlChangeValue",
            channel = 15,
            controller_number = 81,
            character = "Button",
            fourteen_bit = false,
        },
        glue = {
            absolute_mode = "IncrementalButton",
            reverse = true,
            step_size_interval = { 0.010101010101010102, 0.010101010101010102 },
            fire_mode = turbo_mode,
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 1,
            },
        },
    },
    {
        name = "Up LED - Previous macro bank",
        group = "macro-banks",
        control_enabled = false,
        source = {
            kind = "MidiControlChangeValue",
            channel = 15,
            controller_number = 81,
            character = "Button",
            fourteen_bit = false,
        },
        glue = {
            source_interval = { 0, 0.94 },
            target_interval = { 0, 0.010101010101010102 },
            step_size_interval = { 0.010101010101010102, 0.010101010101010102 },
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 1,
            },
        },
    },
    {
        name = "Down - Next macro bank",
        group = "macro-banks",
        feedback_enabled = false,
        source = {
            kind = "MidiControlChangeValue",
            channel = 15,
            controller_number = 82,
            character = "Button",
            fourteen_bit = false,
        },
        glue = {
            absolute_mode = "IncrementalButton",
            step_size_interval = { 0.010101010101010102, 0.010101010101010102 },
            fire_mode = turbo_mode,
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 1,
            },
        },
    },
    {
        name = "Macro bank display",
        group = "macro-banks",
        control_enable = false,
        source = {
            kind = "MidiScript",
            script_kind = "lua",
            script = reusable_lua_code .. [[
local column = 8
local object = 1
return {
    address = column * 8 + object,
    messages = {
        create_screen_props_msg({
            create_text_prop_change(8, object, y),
        }),
    }
} ]],
        },
        glue = {
            feedback = {
                kind = "Text",
                text_expression = "Bank: {{ target.text_value }}",
            },
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 1,
            },
        },
    },
}

for i = 0, 7 do
    local human_i = i + 1
    local param_expression = "mapped_fx_parameter_indexes[p[1] * 8 + " .. i .. "]"
    local param_value_control_mapping = {
        name = "Encoder " .. human_i .. ": Macro control " .. human_i,
        group = "macro-parameters",
        feedback_enabled = false,
        source = {
            kind = "MidiControlChangeValue",
            channel = 15,
            controller_number = 21 + i,
            character = "Relative1",
            fourteen_bit = false,
        },
        glue = {
            step_size_interval = { 0.01, 0.05 },
            step_factor_interval = { 1, 5 },
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "Dynamic",
                fx = {
                    address = "Instance",
                },
                expression = param_expression,
            },
        },
    }
    local param_value_feedback_mapping = {
        name = "Encoder " .. human_i .. ": Macro feedback " .. human_i,
        group = "macro-parameters",
        control_enabled = false,
        source = {
            kind = "MidiScript",
            script_kind = "lua",
            script = reusable_lua_code .. [[
local column = ]] .. i .. [[

local color_offset = 1000
local object = 1
local color = y and white or black
local value = y and math.floor(y * 127) or 0
return {
    address = color_offset + column * 8 + object,
    messages = {
        create_screen_props_msg({
            -- Make the knob visible (by making it white)
            create_rgb_color_prop_change(column, object, color),
            -- Rotate the knob so it reflects the parameter value
            create_value_prop_change(column, 0, value),
        }),
    }
}]],
        },
        glue = {
            step_size_interval = { 0.01, 0.05 },
            step_factor_interval = { 1, 5 },
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "Dynamic",
                fx = {
                    address = "Instance",
                },
                expression = param_expression,
            },
        },
    }
    local section_name_mapping = {
        name = "Screen " .. human_i .. ": Section " .. human_i .. " name",
        group = "macro-parameters",
        control_enabled = false,
        source = {
            kind = "MidiScript",
            script_kind = "lua",
            script = reusable_lua_code .. [[
local column = ]] .. i .. [[

local object = 0
return {
    address = column * 8 + object,
    messages = {
        create_screen_props_msg({
            create_text_prop_change(column, object, y),
        }),
    }
}]],
        },
        glue = {
            step_size_interval = { 0.01, 0.05 },
            step_factor_interval = { 1, 5 },
            feedback = {
                kind = "Text",
                text_expression = "{{ target.fx_parameter.macro.new_section.name }}",
            },
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "Dynamic",
                fx = {
                    address = "Instance",
                },
                expression = param_expression,
            },
        },
    }
    local macro_name_mapping = {
        name = "Screen " .. human_i .. ": Macro " .. human_i .. " name",
        group = "macro-parameters",
        control_enabled = false,
        source = {
            kind = "MidiScript",
            script_kind = "lua",
            script = reusable_lua_code .. [[
local column = ]] .. i .. [[

local object = 1
return {
    address = column * 8 + object,
    messages = {
        create_screen_props_msg({
            create_text_prop_change(column, object, y),
        }),
    }
}]],
        },
        glue = {
            step_size_interval = { 0.01, 0.05 },
            step_factor_interval = { 1, 5 },
            feedback = {
                kind = "Text",
                text_expression = "{{ target.fx_parameter.macro.name }}",
            },
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "Dynamic",
                fx = {
                    address = "Instance",
                },
                expression = param_expression,
            },
        },
    }
    local param_name_mapping = {
        name = "Screen " .. human_i .. ": Param " .. human_i .. " name",
        group = "macro-resolved-parameters",
        control_enabled = false,
        source = {
            kind = "MidiScript",
            script_kind = "lua",
            script = reusable_lua_code .. [[
local column = ]] .. i .. [[

local object = 3
return {
    address = column * 8 + object,
    messages = {
        create_screen_props_msg({
            create_text_prop_change(column, object, y),
        }),
    }
}]],
        },
        glue = {
            step_size_interval = { 0.01, 0.05 },
            step_factor_interval = { 1, 5 },
            feedback = {
                kind = "Text",
                text_expression = "{{ target.fx_parameter.name }}",
            },
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "Dynamic",
                fx = {
                    address = "Instance",
                },
                expression = param_expression,
            },
        },
    }
    local param_value_label_mapping = {
        name = "Screen 1: Macro 1 value",
        group = "macro-parameters",
        control_enabled = false,
        source = {
            kind = "MidiScript",
            script_kind = "lua",
            script = reusable_lua_code .. [[
local column = ]] .. i .. [[

local object = 2
return {
    address = column * 8 + object,
    messages = {
        create_screen_props_msg({
            create_text_prop_change(column, object, y),
        }),
    }
} ]],
        },
        glue = {
            step_size_interval = { 0.01, 0.05 },
            step_factor_interval = { 1, 5 },
            feedback = {
                kind = "Text",
            },
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "Dynamic",
                fx = {
                    address = "Instance",
                },
                expression = param_expression,
            },
        },
    }
    table.insert(mappings, param_value_control_mapping)
    table.insert(mappings, param_value_feedback_mapping)
    table.insert(mappings, section_name_mapping)
    table.insert(mappings, param_name_mapping)
    table.insert(mappings, param_value_label_mapping)
    table.insert(mappings, macro_name_mapping)
end

return {
    kind = "MainCompartment",
    version = "2.15.0",
    value = {
        parameters = parameters,
        groups = groups,
        mappings = mappings,
    },
}