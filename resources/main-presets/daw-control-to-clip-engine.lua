-- ### Configuration ###


-- ### Content ###

local parameters = {
    {
        index = 0,
        name = "Column offset",
    },
    {
        index = 1,
        name = "Row offset",
    },
}

local groups = {
    {
        id = "slot-play",
        name = "Slot play",
    },
}

local column_expression = "p[0] * 100"
local row_expression = "p[1] * 100"

local mappings = {
    {
        id = "slot-play",
        name = "Slot play",
        group = "slot-play",
        source = {
            kind = "Virtual",
            character = "Button",
            id = "play",
        },
        glue = {
            absolute_mode = "ToggleButton",
        },
        target = {
            kind = "ClipTransportAction",
            slot = {
                address = "Dynamic",
                column_expression = column_expression,
                row_expression = row_expression,
            },
            action = "PlayStop",
        },
    },
    {
        id = "position",
        name = "Position",
        control_enabled = false,
        source = {
            kind = "Virtual",
            id = "ch1/lcd/line1",
        },
        glue = {
            feedback = {
                kind = "Text",
            },
        },
        target = {
            kind = "ClipSeek",
            slot = {
                address = "Dynamic",
                column_expression = column_expression,
                row_expression = row_expression,
            },
            feedback_resolution = "High",
        },
    },
    {
        id = "volume",
        name = "Volume",
        source = {
            kind = "Virtual",
            id = "ch1/fader",
        },
        target = {
            kind = "ClipVolume",
            slot = {
                address = "Dynamic",
                column_expression = column_expression,
                row_expression = row_expression,
            },
        },
    },
    {
        id = "NjmzrUDIo-EgoOxRMpBk-",
        name = "Col <",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "bank-left",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            reverse = true,
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
        id = "XEhlV0MCzkK8cNKupBJry",
        name = "Col >",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "bank-right",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
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
        id = "YfPj7gMNNTwhqufds9REa",
        name = "Row <",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "ch-left",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            reverse = true,
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
        id = "EYJC65-wDyclogn8HOoxe",
        name = "Row >",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "ch-right",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
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

return {
    kind = "MainCompartment",
    value = {
        parameters = parameters,
        groups = groups,
        mappings = mappings,
    },
}