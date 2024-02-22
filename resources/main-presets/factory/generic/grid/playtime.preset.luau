--- name: Generic grid controller - Playtime
--- realearn_version: 2.16.0-pre.8
--- author: helgoboss
--- description: |
---     This main preset turns any grid-like controller into a device for doing very basic clip launching via Playtime.
---
---     This generic preset is intended to be used as fallback if no device-specific preset exists. It lacks features because it can't take advantage
---     of device-specific features.
--- used_schemes: [grid]
--- required_features: [playtime]

--!strict

-- Configuration

local stop_column_if_slot_empty = true

-- Requires

local realearn = require("realearn")

-- Constants

-- TODO This should be parameterized from the controller preset. One more reason to allow script parameterization :)

local column_count = 4
local row_count = 4

-- Mappings

local mappings = {}

-- For each slot
for col = 0, column_count - 1 do
    for row = 0, row_count - 1 do
        local mapping = realearn.Mapping {
            name = `Trigger slot {col + 1}/{row + 1}`,
            source = realearn.Source.Virtual {
                character = "Button",
                id = `col{col + 1}/row{row + 1}/pad`,
            },
            glue = {
                feedback = realearn.Feedback.Text {
                    text_expression = "{{ target.slot_state.id }}",
                    color = realearn.VirtualColor {
                        prop = "target.slot.color",
                    },
                },
            },
            target = realearn.Target.PlaytimeSlotTransportAction {
                slot = realearn.PlaytimeSlotDescriptor.Dynamic {
                    column_expression = `control_unit_column_index + {col}`,
                    row_expression = `control_unit_row_index + {row}`,
                },
                action = "Trigger",
                stop_column_if_slot_empty = stop_column_if_slot_empty,
            },
        }
        table.insert(mappings, mapping)
    end
end

return realearn.Compartment {
    mappings = mappings :: any,
    custom_data = {
        playtime = {
            control_unit = {
                column_count = column_count,
                row_count = row_count,
            },
        },
    },
}
