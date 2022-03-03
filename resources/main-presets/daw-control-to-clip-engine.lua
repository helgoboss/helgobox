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
                column_expression = "p[0] * 100",
                row_expression = "p[1] * 100",
            },
            action = "PlayStop",
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
            step_size_interval = { 0.01, 0.05 },
            step_factor_interval = { 1, 5 },
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
            step_size_interval = { 0.01, 0.05 },
            step_factor_interval = { 1, 5 },
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
            step_size_interval = { 0.01, 0.05 },
            step_factor_interval = { 1, 5 },
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
            step_size_interval = { 0.01, 0.05 },
            step_factor_interval = { 1, 5 },
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