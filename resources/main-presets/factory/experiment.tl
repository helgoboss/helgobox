local type MyEnum = enum
   "north"
   "south"
   "east"
   "west"
end

local record Point
   xani: number
   y: number
end

local a: string | number | Point
local b: {boolean} | MyEnum
local c: number | {string:number}

local function Mapping(value: Point): Point
   -- ...
end

local rec: Point = {
    
}