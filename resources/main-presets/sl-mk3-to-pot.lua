-- ## Constants ##

local reusable_lua_code = [[
-- ## Constants ##

local black = { r = 0, g = 0, b = 0 }
local white = { r = 255, g = 255, b = 255 }

-- ## Functions ##

function to_ascii(text)
    local bytes = {}
    if text ~= nil then
        for i = 1, string.len(text) do
            local byte = string.byte(text, i)
            if byte < 128 then
                table.insert(bytes, byte)
            end
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

-- ## Functions ##

function concat_table(t1, t2)
    for i = 1, #t2 do
        t1[#t1 + 1] = t2[i]
    end
end

function create_browse_mappings(title, column, target)
    local human_column = column + 1
    return {
        {
            name = "Encoder " .. human_column .. " - Browse products",
            group = "browse-columns",
            source = {
                kind = "MidiControlChangeValue",
                channel = 15,
                controller_number = 21 + column,
                character = "Relative1",
                fourteen_bit = false,
            },
            glue = {
                step_factor_interval = { -10, 5 },
                wrap = true,
            },
            target = target,
        },
        {
            name = "Screen " .. human_column .. " - Browse products",
            group = "browse-columns",
            source = {
                kind = "MidiScript",
                script_kind = "lua",
                script = reusable_lua_code .. [[
local column = ]] .. column .. [[

local object = 0
local part_1 = y and string.sub(y, 1, 9) or nil
local part_2 = y and string.sub(y, 10, 18) or nil
return {
    address = column * 9 + object,
    messages = {
        create_screen_props_msg({
            --create_rgb_color_prop_change(column, object, white),
            --create_value_prop_change(column, object, 1),
            --create_text_prop_change(column, object, "]]..title..[["),
            create_text_prop_change(column, object + 0, part_1),
            create_text_prop_change(column, object + 1, part_2),
        }),
    }
} ]],
            },
            glue = {
                feedback = {
                    kind = "Text",
                },
            },
            target = target,
        }
    }
end

-- ## Code ##

local parameters = {
    {
        index = 0,
        name = "Mode",
        value_count = 10,
    },
    {
        index = 1,
        name = "Macro bank",
        value_count = 100,
    },
}
local browse_mode_condition = {
    kind = "Bank",
    parameter = 0,
    bank_index = 0,
}
local macro_mode_condition = {
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
        id = "browse-columns",
        name = "Browse columns",
        activation_condition = browse_mode_condition,
    },
    {
        id = "browse-actions",
        name = "Browse actions",
        activation_condition = browse_mode_condition,
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
            source_interval = { 0.16, 1 },
            target_interval = { 0, 0.1111111111111111 },
            wrap = true,
            step_size_interval = { 0.1111111111111111, 0.1111111111111111 },
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
        name = "Init macro mode",
        group = "modes",
        activation_condition = macro_mode_condition,
        source = {
            kind = "MidiScript",
            script_kind = "lua",
            script = reusable_lua_code .. [[

return {
    address = 1000,
    messages = {
        create_notification_text_msg("Initializing", "macro mode"),
        create_screen_layout_msg(1),
        create_screen_props_msg({
            create_text_prop_change(8, 0, y and "Macros" or nil),
        }),
    }
} ]],
        },
        target = {
            kind = "Dummy",
        },
    },
    {
        name = "Init browse mode",
        group = "modes",
        activation_condition = browse_mode_condition,
        source = {
            kind = "MidiScript",
            script_kind = "lua",
            script = reusable_lua_code .. [[

return {
    address = 1000,
    messages = {
        create_notification_text_msg("Initializing", "browse mode"),
        create_screen_layout_msg(2),
        create_screen_props_msg({
            create_text_prop_change(8, 0, y and "Browse" or nil),
        }),
    }
} ]],
        },
        target = {
            kind = "Dummy",
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
    address = column * 9 + object,
    messages = {
        create_screen_props_msg({
            create_text_prop_change(column, object, y),
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
    {
        name = "Preview preset",
        group = "browse-actions",
        feedback_enabled = false,
        source = {
            kind = "MidiControlChangeValue",
            channel = 15,
            controller_number = 57,
            character = "Button",
            fourteen_bit = false,
        },
        glue = {
            step_size_interval = { 0.01, 0.05 },
            button_filter = "PressOnly",
        },
        target = {
            kind = "PreviewPotPreset",
        },
    },
    {
        name = "Preview preset feedback",
        group = "browse-actions",
        control_enabled = false,
        source = {
            kind = "MidiControlChangeValue",
            channel = 15,
            controller_number = 57,
            character = "Button",
            fourteen_bit = false,
        },
        glue = {
            source_interval = { 0.1, 1 },
            step_size_interval = { 0.01, 0.05 },
            button_filter = "PressOnly",
        },
        target = {
            kind = "Dummy",
        },
    },
    {
        name = "Load preset",
        group = "browse-actions",
        feedback_enabled = false,
        source = {
            kind = "MidiControlChangeValue",
            channel = 15,
            controller_number = 58,
            character = "Button",
            fourteen_bit = false,
        },
        glue = {
            step_size_interval = { 0.01, 0.05 },
            button_filter = "PressOnly",
        },
        target = {
            kind = "LoadPotPreset",
            fx = {
                address = "ByIndex",
                chain = {
                    address = "Track",
                    track = {
                        address = "Selected",
                    },
                },
                index = 0,
            },
        },
    },
    {
        name = "Load preset feedback",
        group = "browse-actions",
        control_enabled = false,
        source = {
            kind = "MidiControlChangeValue",
            channel = 15,
            controller_number = 58,
            character = "Button",
            fourteen_bit = false,
        },
        glue = {
            source_interval = { 0.45, 1 },
            step_size_interval = { 0.01, 0.05 },
            button_filter = "PressOnly",
        },
        target = {
            kind = "Dummy",
        },
    },
}

-- One browser per column
concat_table(
        mappings,
        create_browse_mappings("Database", 0, {
            kind = "BrowsePotFilterItems",
            item_kind = "Database",
        })
)
concat_table(
        mappings,
        create_browse_mappings("Kind", 1, {
            kind = "BrowsePotFilterItems",
            item_kind = "ProductKind",
        })
)
concat_table(
        mappings,
        create_browse_mappings("Product", 2, {
            kind = "BrowsePotFilterItems",
            item_kind = "Bank",
        })
)
concat_table(
        mappings,
        create_browse_mappings("Bank", 3, {
            kind = "BrowsePotFilterItems",
            item_kind = "SubBank",
        })
)
concat_table(
        mappings,
        create_browse_mappings("Category", 4, {
            kind = "BrowsePotFilterItems",
            item_kind = "Category",
        })
)
concat_table(
        mappings,
        create_browse_mappings("->", 5, {
            kind = "BrowsePotFilterItems",
            item_kind = "SubCategory",
        })
)
concat_table(
        mappings,
        create_browse_mappings("Character", 6, {
            kind = "BrowsePotFilterItems",
            item_kind = "Mode",
        })
)
concat_table(
        mappings,
        create_browse_mappings("Preset", 7, {
            kind = "BrowsePotPresets",
        })
)

-- One macro parameter per column
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
    address = color_offset + column * 9 + object,
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
    address = column * 9 + object,
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
    address = column * 9 + object,
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
    address = column * 9 + object,
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
    address = column * 9 + object,
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