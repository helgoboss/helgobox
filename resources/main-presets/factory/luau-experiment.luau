--!strict

type LivingBeing_Person = { kind: "person", firstName: string, lastName: string? }

type Person = {
	--- First name of this person
	firstName: string,
	lastName: string?,
}

type LivingBeing_Animal = { kind: "animal", color: string }
type Animal = {
	--- Color of the animal
	color: string,
}

type LivingBeing = LivingBeing_Animal | LivingBeing_Person

local LivingBeing = {}

function LivingBeing.Person(value: Person): LivingBeing_Person
	return {
		kind = "person",
		firstName = value.firstName,
		lastName = value.lastName,
	}
end

function LivingBeing.Animal(value: Animal): LivingBeing_Animal
	return {
		kind = "animal",
		color = value.color,
	}
end

local p = LivingBeing.Person {
    firstName = "d",
    lastName = "bob"
}

local a = LivingBeing.Animal({ color = "red" })

LivingBeing.Animal({
	color = "red",
	obsol = "",
})

local function getRandomLivingBeing(): LivingBeing 
    return LivingBeing.Animal { color = "red"}
end

local b = getRandomLivingBeing()

if b.kind == "animal" then
    b.color = "yellow"
end