--!strict

local realearn = require("realearn")
local util = require("util")

local playtime_util = {}

--- A partial mapping that enables turbo mode for buttons.
function playtime_util.turbo()
    return util.partial_mapping {
        glue = {
            fire_mode = realearn.FireMode.AfterTimeoutKeepFiring {
                timeout = 0,
                rate = 100,
            },
        },
    }
end

local function scroll(axis: realearn.Axis, amount: number)
    local abs_amount = math.abs(amount)
    return util.partial_mapping {
        glue = {
            absolute_mode = "IncrementalButton",
            step_factor_interval = { abs_amount, abs_amount },
            reverse = amount < 0,
			--- This makes the LED switch off when no scrolling is possible in the given direction
            feedback = realearn.Feedback.Numeric {
                transformation = "x = y < 1",
            },
        },
        target = realearn.Target.PlaytimeControlUnitScroll {
            axis = axis,
        },
    }
end

function playtime_util.scroll_horizontally(amount: number)
    return scroll("X", amount)
end

function playtime_util.scroll_vertically(amount: number)
    return scroll("Y", amount)
end


function playtime_util.slot_state_text_feedback()
    return util.partial_mapping {
        glue = {
            feedback = realearn.Feedback.Text {
                text_expression = "{{ target.slot_state.id }}",
                color = realearn.VirtualColor {
                    prop = "target.slot.color",
                },
            },
        },
    }
end

return playtime_util
