--- name: Numbered - FX parameters
--- realearn_version: 2.16.0-pre.9
--- author: helgoboss
--- description: |
---     This main preset is built for controllers that offer a set of knobs and buttons. Each knob will control one parameter of
---     the unit FX (which by default resolves to the currently focused FX), starting with parameter 1. Each button will switch
---     to the next bank of parameters. If the controller has `bank-left` and `bank-right` buttons, they can be used to switch
---     between pages, making it possible to control even more parameters.
---     
---     At the moment, this preset assumes that the controller has exactly 16 knobs and buttons. In future, this will be variable.
--- used_schemes: [numbered]

--!strict

local realearn = require("realearn")

--- Configuration

local multi_count = 16
local button_count = 16

-- Code

local page_count = 16
local bank_count = button_count

local parameters: { realearn.Parameter } = {
    {
        index = 0,
        name = "Page",
        value_count = page_count,
    },
    {
        index = 1,
        name = "Bank",
        value_count = bank_count,
    },
}

local mappings: { realearn.Mapping } = {
    -- Side button left => Decrease page
    realearn.Mapping {
        name = "Page -",
        source = realearn.Source.Virtual {
            character = "Button",
            id = "bank-left",
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
        source = realearn.Source.Virtual {
            character = "Button",
            id = "bank-right",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            reverse = false,
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
        source = realearn.Source.Virtual {
            character = "Multi",
            id = i,
        },
        target = realearn.Target.FxParameterValue {
            parameter = realearn.FxParameterDescriptor.Dynamic {
                fx = realearn.FxDescriptor.Instance {},
                expression = `p[0] * {page_count} * {bank_count} + p[1] * {bank_count} + {i}`,
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
        source = realearn.Source.Virtual {
            character = "Button",
            id = i,
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
