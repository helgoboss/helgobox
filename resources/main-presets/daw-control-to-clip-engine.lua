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
                column_expression = "0",
                row_expression = "0"
            },
            action = "PlayStop",
        },
    }
}

return {
    kind = "MainCompartment",
    value = {
        parameters = parameters,
        groups = groups,
        mappings = mappings,
    },
}