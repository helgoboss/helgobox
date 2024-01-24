--- name: Luau example preset
--- author: helgoboss
--- realearn_version: 2.16.0-pre.8
--!strict

local realearn = require("realearn")
-- local realearn = require("realearn.luau")
local Partial = require("Partial")

local a = Partial.new({
	name = "Sustain",
})
local b = Partial.new({
	feedback_enabled = false,
})

--- @param value Person
function Mapping(value: realearn.Mapping): realearn.Mapping
    return value
end


local m = Mapping {
   id = "5",
   name = "hallo",
   target = {
	  kind = "TrackArmState",
   }
}
if m.target and m.target.kind == "GoToBookmark" then
	m.target.bookmark
end
local mappings: { realearn.Mapping } = {
	{
		id = "22050182-480e-4267-b203-9ad641a72b44",
		name = "Sustain",
		feedback_enabled = false,
		target = {
			kind = "Virtual",
			id = "bla",
			character = "Button",
		},
	},
}

local mappings2: { realearn.Mapping } = {
	{
		id = "66HyizAcwPic9-35mpwEX",
		name = "1",
		source = {
			kind = "Virtual",
			id = "col1/row1/knob",
		},
		glue = {
			step_size_interval = { 0.01, 0.05 },
			step_factor_interval = { 1, 5 },
		},
		target = {
			kind = "FxParameterValue",
			parameter = {
				address = "ById",
				fx = {
					address = "Instance",
				},
				index = 3,
			},
		}
	},
	{
		id = "cY-Qtxhjgd9Z1y6pUlKWd",
		name = "2",
		source = {
			kind = "Virtual",
			id = "col2/row1/knob",
		},
		glue = {
			step_size_interval = { 0.01, 0.05 },
			step_factor_interval = { 1, 5 },
		},
		target = {
			kind = "FxParameterValue",
			parameter = {
				address = "ById",
				fx = {
					address = "Instance",
				},
				index = 4,
			},
		},
	},
	{
		id = "VRS1j65prM199sRyiU7MW",
		name = "3",
		source = {
			kind = "Virtual",
			id = "col3/row1/knob",
		},
		glue = {
			step_size_interval = { 0.01, 0.05 },
			step_factor_interval = { 1, 5 },
		},
		target = {
			kind = "FxParameterValue",
			parameter = {
				address = "ById",
				fx = {
					address = "Instance",
				},
				index = 5,
			},
		},
	},
	{
		id = "9wr9u-V-la7tIS6THOSw5",
		name = "4",
		source = {
			kind = "Virtual",
			id = "col4/row1/knob",
		},
		glue = {
			step_size_interval = { 0.01, 0.05 },
			step_factor_interval = { 1, 5 },
		},
		target = {
			kind = "FxParameterValue",
			parameter = {
				address = "ById",
				fx = {
					address = "Instance",
				},
				index = 6,
			},
		},
	},
	{
		id = "Qelg13M9p75Yn4qDbtJJz",
		name = "5",
		source = {
			kind = "Virtual",
			id = "col1/row2/knob",
		},
		glue = {
			step_size_interval = { 0.01, 0.05 },
			step_factor_interval = { 1, 5 },
		},
		target = {
			kind = "FxParameterValue",
			parameter = {
				address = "ById",
				fx = {
					address = "Instance",
				},
				index = 7,
			},
		},
	},
	{
		id = "tikqGRe_ONtY-fgeECdQm",
		name = "6",
		source = {
			kind = "Virtual",
			id = "col2/row2/knob",
		},
		glue = {
			step_size_interval = { 0.01, 0.05 },
			step_factor_interval = { 1, 5 },
		},
		target = {
			kind = "FxParameterValue",
			parameter = {
				address = "ById",
				fx = {
					address = "Instance",
				},
				index = 8,
			},
		},
	},
	{
		id = "tJRsdURYS98mMr_ZaWWlt",
		name = "7",
		source = {
			kind = "Virtual",
			id = "col3/row2/knob",
		},
		glue = {
			step_size_interval = { 0.01, 0.05 },
			step_factor_interval = { 1, 5 },
		},
		target = {
			kind = "FxParameterValue",
			parameter = {
				address = "ById",
				fx = {
					address = "Instance",
				},
				index = 9,
			},
		},
	},
	{
		id = "SgW1j34viPN8zGLWYArK8",
		name = "8",
		source = {
			kind = "Virtual",
			id = "col4/row2/knob",
		},
		glue = {
			step_size_interval = { 0.01, 0.05 },
			step_factor_interval = { 1, 5 },
		},
		target = {
			kind = "FxParameterValue",
			parameter = {
				address = "ById",
				fx = {
					address = "Instance",
				},
				index = 10,
			},
		},
	},
	{
		id = "SRUwICa8J_LkFeoozWq4r",
		name = "9",
		source = {
			kind = "Virtual",
			id = "col3/row2/knob",
		},
		glue = {
			step_size_interval = { 0.01, 0.05 },
			step_factor_interval = { 1, 5 },
		},
		target = {
			kind = "FxParameterValue",
			parameter = {
				address = "ById",
				fx = {
					address = "Instance",
				},
				index = 11,
			},
		},
	},
}

return {
	mappings = mappings,
}
