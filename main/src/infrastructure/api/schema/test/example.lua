return {
    name = "Pedal to Delay",
    source = {
        kind = "MidiControlChangeValue",
        channel = 0,
        controller_number = 64,
        character = "Button",
        fourteen_bit = false,
    },
    glue = {
        target_interval = {0, 0.53},
        jump_interval = {0, 0.53},
        step_size_interval = {0.01, 0.01},
        step_factor_interval = {1, 1},
    },
    target = {
        kind = "FxParameterValue",
        parameter = {
            address = "ById",
            fx = {
                address = "ById",
                chain = {
                    address = "Track",
                },
                id = "22FD4FC0-A4DD-4E6F-BCB3-38F242B557B2",
            },
            index = 23,
        },
    },
}