local playtime = require("playtime")

local column_count = 10
local row_count = 200

-- Build columns
local columns = {}
local clip_index = 0
for c = 0, column_count - 1 do
    local slots: { playtime.Slot } = {}
    for r = 0, row_count - 1 do
        if (c % 2 == 0 and r % 2 == 0) or (c % 2 == 1 and r % 2 == 1) then
            local slot = playtime.Slot {
                row = r,
                clips = {
                    playtime.Clip {
                        name = `Clip {c + 1}/{r + 1}`,
                        color = playtime.ClipColor.PaletteColor {
                            index = 5 + clip_index % 17,
                        },
                    },
                },
            }
            table.insert(slots, slot)
            clip_index += 1
        end
    end
    local column = playtime.Column {
        slots = slots or nil,
    }
    table.insert(columns, column)
end

-- Build rows
local rows = {}
for r = 1, row_count do
    local row = playtime.Row {
        name = `Scene {r}`,
    }
    table.insert(rows, row)
end

-- Build matrix
local matrix = playtime.Matrix {
    columns = columns,
    rows = rows,
}

-- Return result
return {
    kind = "ClipMatrix",
    version = "2.16.14",
    value = matrix,
}
