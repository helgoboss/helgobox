--- name: FX parameters

--!strict

local realearn = require("realearn")

local parameters: { realearn.Parameter } = {
    {
        index = 0,
        name = "Page",
        value_count = 16,
    },
    {
        index = 1,
        name = "Bank",
        value_count = 16,
    },
}

local mappings: { realearn.Mapping } = {
    -- Side button left => Decrease page
    realearn.Mapping {
        name = "Page -",
        source = realearn.Source.MidiControlChangeValue {
            channel = 3,
            controller_number = 9,
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            reverse = true,
        },
        target = realearn.Target.CompartmentParameterValue {
            parameter = realearn.CompartmentParameterDescriptor.ById {
                index = 0,
            },
        },
    },
    -- Side button right => Increase page
    realearn.Mapping {
        name = "Page +",
        source = realearn.Source.MidiControlChangeValue {
            channel = 3,
            controller_number = 12,
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
        },
        target = realearn.Target.CompartmentParameterValue {
            parameter = realearn.CompartmentParameterDescriptor.ById {
                index = 0,
            },
        },
    },
}

-- Encoder movement => Set FX parameter value
for i = 0, 15 do
    local m = realearn.Mapping {
        name = `Set parameter value {i + 1}`,
        source = realearn.Source.MidiControlChangeValue {
            channel = 0,
            controller_number = i,
            character = "Relative2",
        },
        target = realearn.Target.FxParameterValue {
            parameter = realearn.FxParameterDescriptor.Dynamic {
                fx = realearn.FxDescriptor.Instance {},
                expression = `p[0] * 16 * 16 + p[1] * 16 + {i}`,
            },
        },
    }
    table.insert(mappings, m)
end

-- Encoder push => Select bank
for i = 0, 15 do
    local normalized_bank = i / 15
    local m = realearn.Mapping {
        name = `Switch to bank {i + 1}`,
        source = realearn.Source.MidiControlChangeValue {
            channel = 1,
            controller_number = i,
            character = "Button",
        },
        glue = {
            out_of_range_behavior = "Min",
            target_interval = { normalized_bank, normalized_bank },
        },
        target = realearn.Target.CompartmentParameterValue {
            parameter = realearn.CompartmentParameterDescriptor.ById {
                index = 1,
            },
        },
    }
    table.insert(mappings, m)
end

return realearn.Compartment {
    parameters = parameters,
    mappings = mappings,
}
