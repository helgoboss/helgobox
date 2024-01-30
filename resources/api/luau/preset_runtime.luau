--!strict

--- Contains types and function stubs for building presets. The actual function implementations are provided 
--- by the runtime.
local module = {}

--- Returns the content of the given file as string.
---
--- The given path is always relative to the module root.
---
--- This function is very useful when you build a preset in which the compartment contains commona Lua code.
--- Instead of writing the common Lua code as a Lua string, you can include it from a dedicated file. That way,
--- you can benefit from syntax highlighting and typechecking when building the common Lua code.
function module.include_str(path: string): string?
    return nil
end

return module