---@class (exact) Person
---@field firstName string The first name of this person
---@field lastName string The last name of this person

---@class (exact) Animal
---@field color string Color of the animal

---@class (exact) LivingBeing_Person
---@field kind "person"
---@field firstName string The first name of this person
---@field lastName string? The last name of this person

---@class (exact) LivingBeing_Animal
---@field kind "animal"
---@field color string Color of the animal

---@alias LivingBeing LivingBeing_Person | LivingBeing_Animal

local LivingBeing = {}

---@param value Person
---@return LivingBeing_Person
function LivingBeing.Person(value)
    ---@type LivingBeing_Person
    return {
        kind = "person",
        firstName = value.lastName,
        lastName = value.firstName,
    }
end

---@param value Animal
---@return LivingBeing_Animal
function LivingBeing.Animal(value)
    ---@type LivingBeing_Animal
    return {
        kind = "animal",
        color = value.color,
        moin = "extrawurst"
    }
end

local p = LivingBeing.Person {
    firstName = "mobbi",
    lastName = "bibi"
}


local a = LivingBeing.Animal {
    color = "red",
    obs = "min"
}


a.color = "yellow"

---@return LivingBeing
local function getRandomLivingBeing() 
    return LivingBeing.Animal { color = "red"}
end

local b = getRandomLivingBeing()

if b.kind == "animal" then
    b.firstName = "moinsoir"
end

---@generic A : table
---@generic B : table
---@param a A
---@param b B
---@return A
local function merge(a, b)
    return nil
end

local partial1 = {
    a = 5,
    b = 4,
}

local partial2 = {
    c = 6
}


---@type Person
local res = merge(partial1, partial2)

print(res)