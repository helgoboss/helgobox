-- ## Constants ##

function color(hex)
    hex = hex:gsub("#", "")
    return {
        r = tonumber("0x" .. hex:sub(1, 2)),
        g = tonumber("0x" .. hex:sub(3, 4)),
        b = tonumber("0x" .. hex:sub(5, 6)),
    }
end

-- Palette

local black = color("#000000")
local white = color("#ffffff")
local palette = {
    color("#2196f3"),
    color("#795548"),
    color("#ff5722"),
    color("#ffeb3b"),
    color("#4caf50"),
    color("#304ffe"),
    color("#b71c1c"),
    color("#9c27b0"),
}


-- Browser colors
local browse_dbs_color = palette[1]
local browse_products_color = palette[2]
local browse_types_color = palette[3]
local browse_characters_color = palette[4]
local browse_presets_color = palette[5]

-- Action colors
local filter_action_color = white
local preview_action_color = palette[6]
local load_action_color = palette[7]

-- Macro colors
local macro_colors = palette

-- Targets
local load_preset_target = {
    kind = "LoadPotPreset",
    fx = {
        address = "Instance"
    },
}

-- More
local reusable_lua_code = [[
-- ## Constants ##

local column_knob_address_offset = 5000
local led_address_offset = 10000
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
    table.insert(bytes, 0)
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

--- Creates a message for changing the state of an LED.
---
--- Passing a `nil` color will make the LED go off.
---
--- @param led_index number which LED to talk to
--- @param led_behavior number LED behavior (1 = solid, 2 = flashing with previously set solid color, 3 = pulsating)
--- @param color table the RGB color (table with properties r, g and b)
function create_led_msg(led_index, led_behavior, color)
    if color == nil then
        color = { r = 0, g = 0, b = 0 }
    end
    return create_msg({
        0x03,
        led_index,
        led_behavior,
        math.floor(color.r / 2),
        math.floor(color.g / 2),
        math.floor(color.b / 2),
    })
end

function create_left_right_animation(global_millis, max_char_count, frame_length, text)
    if text == nil then
        return nil
    end
    if #text > max_char_count then
        local frame_count = #text - max_char_count
        local frame_index = math.floor(global_millis / frame_length) % (frame_count * 2)
        local text_offset
        if frame_index < frame_count then
            text_offset = frame_index
        else
            local distance = frame_index - frame_count
            text_offset = frame_count - distance
        end
        return text:sub(text_offset + 1, text_offset + max_char_count)
    else
        return text
    end
end

-- ## Code ##
]]

-- ## Functions ##
-- https://stackoverflow.com/a/6081639
function serialize_internal(val, name, skipnewlines, depth)
    skipnewlines = skipnewlines or false
    depth = depth or 0
    local tmp = string.rep(" ", depth)
    if name then
        tmp = tmp .. "[" .. serialize(name) .. "]" .. " = "
    end
    if type(val) == "table" then
        tmp = tmp .. "{" .. (not skipnewlines and "\n" or "")
        for k, v in pairs(val) do
            tmp = tmp .. serialize_internal(v, k, skipnewlines, depth + 1) .. "," .. (not skipnewlines and "\n" or "")
        end
        tmp = tmp .. string.rep(" ", depth) .. "}"
    elseif type(val) == "number" then
        tmp = tmp .. tostring(val)
    elseif type(val) == "string" then
        tmp = tmp .. string.format("%q", val)
    elseif type(val) == "boolean" then
        tmp = tmp .. (val and "true" or "false")
    else
        tmp = tmp .. "\"[inserializeable datatype:" .. type(val) .. "]\""
    end
    return tmp
end
function serialize(val)
    if val == nil then
        return "nil"
    end
    return serialize_internal(val, nil, true, nil)
end

function concat_table(t1, t2)
    for i = 1, #t2 do
        t1[#t1 + 1] = t2[i]
    end
end

function create_browse_mappings(title, column, color, action, secondary_prop_key, target)
    local human_column = column + 1
    local color_string = serialize(color)
    local mappings = {
        {
            name = "Encoder " .. human_column .. " - " .. title,
            feedback_enabled = false,
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
                wrap = false,
            },
            target = target,
        },
        {
            name = "Screen " .. human_column .. " - Browse products",
            control_enabled = false,
            group = "browse-columns",
            source = {
                kind = "MidiScript",
                script_kind = "lua",
                script = reusable_lua_code .. [[
local column = ]] .. column .. [[

local action = ]] .. serialize(action) .. [[

local available = y and y.available or false
local label = available and y.label or ""
local name_1 = available and string.sub(y.name, 1, 9) or ""
local name_2 = available and string.sub(y.name, 10, 18) or ""
local name_3 = available and string.sub(y.name, 19, 27) or ""
local progress = (available and y.index and y.count) and ("" .. (y.index + 1) .. "/" .. y.count) or ""
local secondary_prop = available and y.secondary_prop or ""
local color = available and (context.feedback_event.color or white) or black
local action_name = available and action and action.name or nil
local action_color = available and (action and action.color or nil) or black
return {
    address = column,
    messages = {
        create_screen_props_msg({
            -- Header
            create_rgb_color_prop_change(column, 0, color),
            create_value_prop_change(column, 0, 1),
            create_text_prop_change(column, 0, label),
            create_text_prop_change(column, 1, progress),
            -- Content
            create_text_prop_change(column, 2, name_1),
            create_text_prop_change(column, 3, name_2),
            -- Footer
            create_rgb_color_prop_change(column, 2, action_color),
            create_value_prop_change(column, 2, 0),
            create_text_prop_change(column, 4, ""),
            create_text_prop_change(column, 5, action_name or secondary_prop),
        }),
    }
}
]],
            },
            glue = {
                feedback = {
                    kind = "Dynamic",
                    script = [[
local index = context.prop("target.discrete_value")
local count = context.prop("target.discrete_value_count")
local name = context.prop("target.text_value") or "-"
local secondary_prop_key = ]] .. serialize(secondary_prop_key) .. [[

local secondary_prop_value = secondary_prop_key and context.prop(secondary_prop_key) or ""
local available = context.prop("target.available")
return {
    feedback_event = {
        color = ]] .. color_string .. [[,
        value = {
            label = "]] .. title .. [[",
            name = name,
            secondary_prop = secondary_prop_value,
            available = available,
            index = index,
            count = count,
        }
    },
}
]],
                },
            },
            target = target,
        }
    }
    if action ~= nil then
        local button_filter
        local absolute_mode = "Normal"
        if action.control_kind == "trigger" then
            button_filter = "PressOnly"
        elseif action.control_kind == "toggle" then
            absolute_mode = "ToggleButton"
        end
        local control_action_mapping = {
            name = action.name .. " (control)",
            group = "browse-actions",
            feedback_enabled = false,
            source = {
                kind = "MidiControlChangeValue",
                channel = 15,
                controller_number = 51 + column,
                character = "Button",
                fourteen_bit = false,
            },
            glue = {
                absolute_mode = absolute_mode,
                button_filter = button_filter,
            },
            target = action.target,
        }
        local feedback_action_mapping
        if action.control_kind == "trigger" then
            feedback_action_mapping = {
                name = action.name .. " (feedback)",
                group = "browse-actions",
                control_enabled = false,
                source = {
                    kind = "MidiScript",
                    script_kind = "lua",
                    script = reusable_lua_code .. [[
local column = ]] .. column .. [[

local color = (y and y.available) and ]] .. serialize(action.color) .. [[ or nil

local led_index = 4 + column
return {
    address = led_address_offset + led_index,
    messages = {
        create_led_msg(led_index, 1, color),
    }
}
]],
                },
                glue = {
                    feedback = {
                        kind = "Dynamic",
                        script = [[
local available = context.prop("target.available")
return {
    feedback_event = {
        value = {
            available = available,
        }
    },
}
]],
                    },
                },
                target = action.target,
            }
        else
            feedback_action_mapping = {
                name = action.name .. " (feedback)",
                group = "browse-actions",
                control_enabled = false,
                source = {
                    kind = "MidiScript",
                    script_kind = "lua",
                    script = reusable_lua_code .. [[
local column = ]] .. column .. [[

local color = (y and y ~= 0) and ]] .. serialize(action.color) .. [[ or nil

local led_index = 4 + column
return {
    address = led_address_offset + led_index,
    messages = {
        create_led_msg(led_index, 1, color),
    }
}
]],
                },
                target = action.target,
            }

        end
        table.insert(mappings, control_action_mapping)
        table.insert(mappings, feedback_action_mapping)
    end
    return mappings
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
        control_enabled = false,
        group = "modes",
        activation_condition = macro_mode_condition,
        source = {
            kind = "MidiScript",
            script_kind = "lua",
            script = reusable_lua_code .. [[
return {
    address = 1000,
    messages = {
        create_screen_layout_msg(1),
    }
} ]],
        },
        target = {
            kind = "Dummy",
        },
    }, {
        name = "Init browse mode",
        control_enabled = false,
        group = "modes",
        activation_condition = browse_mode_condition,
        source = {
            kind = "MidiScript",
            script_kind = "lua",
            script = reusable_lua_code .. [[
return {
    address = 1000,
    messages = {
        create_screen_layout_msg(2),
    }
} ]],
        },
        target = {
            kind = "Dummy",
        },
    },
    {
        name = "Preset info",
        group = "modes",
        control_enabled = false,
        source = {
            kind = "MidiScript",
            script_kind = "lua",
            script = reusable_lua_code .. [[
local column = 8
local preset_name = y and y.preset_name or ""
local max_char_count = 9
local frame_length = 150
local preset_name_animation = y and create_left_right_animation(y.millis, max_char_count, frame_length, y.preset_name) or ""
local product_name_animation = y and create_left_right_animation(y.millis, max_char_count, frame_length, y.product_name) or ""
return {
    address = column,
    messages = {
        create_screen_props_msg({
            create_text_prop_change(column, 0, preset_name_animation),
            create_text_prop_change(column, 1, product_name_animation),
        }),
    }
} ]],
        },
        glue = {
            feedback = {
                kind = "Dynamic",
                script = [[
local millis = context.prop("global.realearn.time")
local preset_name = context.prop("target.preset.name")
local product_name = context.prop("target.preset.product.name")
return {
    feedback_event = {
        value = {
            preset_name = preset_name,
            product_name = product_name,
            millis = millis,
        }
    },
}
]],
            },
        },
        target = load_preset_target,
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
return {
    address = column + 1,
    messages = {
        create_screen_props_msg({
            create_text_prop_change(column, 2, y),
        }),
    }
} ]],
        },
        glue = {
            feedback = {
                kind = "Dynamic",
                script = [[
local bank = context.prop("target.text_value")
return {
    feedback_event = {
        value = "Bank: " .. (bank + 1)
    },
}
]],
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

-- One browser per column
local favorite_filter_action = {
    name = "Favorite",
    color = filter_action_color,
    control_kind = "toggle",
    target = {
        kind = "BrowsePotFilterItems",
        item_kind = "IsFavorite",
    },
}
local available_filter_action = {
    name = "Available",
    color = filter_action_color,
    control_kind = "toggle",
    target = {
        kind = "BrowsePotFilterItems",
        item_kind = "IsAvailable",
    },
}
local supported_filter_action = {
    name = "Supported",
    color = filter_action_color,
    control_kind = "toggle",
    target = {
        kind = "BrowsePotFilterItems",
        item_kind = "IsSupported",
    },
}
local user_filter_action = {
    name = "User",
    color = filter_action_color,
    control_kind = "toggle",
    target = {
        kind = "BrowsePotFilterItems",
        item_kind = "IsUser",
    },
}
local preview_action = {
    name = "Preview",
    color = preview_action_color,
    control_kind = "trigger",
    target = {
        kind = "PreviewPotPreset",
    },
}
local load_action = {
    name = "Load",
    color = load_action_color,
    control_kind = "trigger",
    target = load_preset_target
}
concat_table(
        mappings,
        create_browse_mappings("Database", 0, browse_dbs_color, available_filter_action, nil, {
            kind = "BrowsePotFilterItems",
            item_kind = "Database",
        })
)
concat_table(
        mappings,
        create_browse_mappings("Kind", 1, browse_dbs_color, supported_filter_action, nil, {
            kind = "BrowsePotFilterItems",
            item_kind = "ProductKind",
        })
)
concat_table(
        mappings,
        create_browse_mappings("Product", 2, browse_products_color, favorite_filter_action, nil, {
            kind = "BrowsePotFilterItems",
            item_kind = "Bank",
        })
)
concat_table(
        mappings,
        create_browse_mappings("Bank", 3, browse_products_color, user_filter_action, nil, {
            kind = "BrowsePotFilterItems",
            item_kind = "SubBank",
        })
)
concat_table(
        mappings,
        create_browse_mappings("Type", 4, browse_types_color, preview_action, nil, {
            kind = "BrowsePotFilterItems",
            item_kind = "Category",
        })
)
concat_table(
        mappings,
        create_browse_mappings("Sub type", 5, browse_types_color, load_action, nil, {
            kind = "BrowsePotFilterItems",
            item_kind = "SubCategory",
        })
)
concat_table(
        mappings,
        create_browse_mappings("Character", 6, browse_characters_color, nil, nil, {
            kind = "BrowsePotFilterItems",
            item_kind = "Mode",
        })
)
concat_table(
        mappings,
        create_browse_mappings("Preset", 7, browse_presets_color, nil, "target.preset.product.name", {
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
    local param_screen_mapping = {
        name = "Screen " .. human_i,
        group = "macro-parameters",
        control_enabled = false,
        source = {
            kind = "MidiScript",
            script_kind = "lua",
            script = reusable_lua_code .. [[
local column = ]] .. i .. [[

local color = y and (context.feedback_event.color or white) or black
local section_name = y and y.section_name or ""
local macro_name = y and y.macro_name or ""
local normalized_param_value = y and y.param_value or 0.0
local midi_param_value = math.floor(normalized_param_value * 127)
local param_name = y and y.param_name or ""
local param_value_label = y and y.param_value_label or ""
return {
    address = column,
    messages = {
        create_screen_props_msg({
            create_rgb_color_prop_change(column, 0, color),
            create_text_prop_change(column, 0, section_name),
            create_text_prop_change(column, 1, macro_name),
            -- Make the knob visible (by making it white)
            create_rgb_color_prop_change(column, 1, color),
            -- Rotate the knob so it reflects the parameter value
            create_value_prop_change(column, 0, midi_param_value),
            create_text_prop_change(column, 2, param_value_label),
            create_text_prop_change(column, 3, param_name),
        }),
    }
} ]],
        },
        glue = {
            feedback = {
                kind = "Dynamic",
                script = [[

local macro_colors = ]] .. serialize(macro_colors) .. [[

local section_index = context.prop("target.fx_parameter.macro.section.index")
local color = section_index and macro_colors[section_index + 1] or nil

return {
    feedback_event = {
        color = color,
        value = {
            section_name = context.prop("target.fx_parameter.macro.new_section.name"),
            macro_name = context.prop("target.fx_parameter.macro.name"),
            param_value = context.prop("y"),
            param_value_label = context.prop("target.text_value"),
            param_name = context.prop("target.fx_parameter.name"),
        }
    },
}
]],
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
    table.insert(mappings, param_screen_mapping)
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