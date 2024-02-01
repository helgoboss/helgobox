# Compartment preset workspace

This folder is a ReaLearn compartment preset workspace. It should be a direct sub folder of the ReaLearn
controller or main preset folder. The purpose of this folder is to contain a set of ReaLearn compartment presets
and provide you with a convenient workspace to develop those presets.

## Choosing a good folder name

If you have created this workspace via ReaLearn, it will have an auto-generated name. As explained further below,
the name of the workspace folder becomes part of the preset ID! Therefore, make sure to do one of the following
things:

- **Either** rename this folder to a proper identifier of your choice (a name with only lowercase letters and dashes,
  e.g. `foo-bar`). A good default choice is to rename it to your operating system login name. If you have already
  saved presets from the ReaLearn user interface, that folder will probably already exist.
- **Or** copy all the files that you need from it to an already existing other workspace folder and delete this one.

## Sharing presets with other users

Each workspace is completely self-contained, that means you can easily share it with other users
without having to worry about possible name conflicts or missing files:

1. You pass the complete workspace folder to the other user (e.g. by zipping and mailing it or by using cloud sharing).
2. The other user presses "Menu → Preset-related → Open preset folder" and puts your workspace folder into `main`
   or `controller` (depending on whether you are sharing main or controller presets, they are very different things).
3. The other user presses "Menu → Preset-related → Reload all presets from disk".

## Directory structure

The meaning of each file in this folder is determined by its file extension.

### Compartment presets

Compartment preset files within this folder can be arbitrarily nested. But be aware that the preset's file path relative
to the controller or main preset folder is at the same time its ID! If you change its location or name, you will make
it a different preset as far as ReaLearn is concerned!

**Example:** If the name of your workspace folder is `john` and you put your preset in `john/foo/daw-control.json`,
the preset ID will be `john/foo/daw-control`.

Tips:
- Factory presets always begin with `factory`. This workspace name is reserved, so you shouldn't use it. There's no way
  to override factory presets.
- Presets can also reside directly in the main/controller preset folder, not in a workspace folder. This was normal
  in the past but is deprecated now. Such presets will show up as "Unsorted" in ReaLearn's preset picker.

Presets can be written in the following different formats.

### Compartment presets in JSON format 

**File extension:** `.json`

ReaLearn will read all files with this extension as compartment presets written in [JSON](https://www.json.org/) format.

A JSON preset is essentially a big data structure that describes the contents of one ReaLearn compartment, such as 
mappings and groups. JSON presets are simple and verbose. They don't contain any logic and can become very 
repetitive.

It's the format in which ReaLearn saves presets whenever you use "Save as..." in the user 
interface. For all other purposes, it's not recommended to use JSON! If you want to code your own presets, 
you should use the Luau format instead.

### Compartment presets in Luau format

**File extension:** `.preset.luau` or `.preset.lua`

ReaLearn will read all files with this extension as compartment presets written in [Luau](https://luau-lang.org/).

A Luau preset is written in a real programming language. It contains a Luau program whose single purpose is to return
a result. And that result should look very similar to the big data structure of JSON presets.

In addition to the actual Luau code, Luau compartment presets contain a meta-data section right at the top,
which contains the name of the preset and other data that can be scanned by ReaLearn before actually loading the
preset by executing its code.

If you have chosen to create this workspace with factory presets, you should find multiple Luau presets in this folder
that can serve as starting point for building your own presets. You will realize that many of them use Luau type 
annotations and helper methods, which enables auto-completion during development and should make the preset code
more understandable. If you want to learn more about this, read the section below.

As an additional help, you can use "Export to clipboard → Export ... compartment as Lua" in ReaLearn. This gives you
a chunk of text formatted as Luau. It doesn't contain any logic, it looks more like a configuration file
but its valid Luau code. This chunk of text is not a ready-made preset (because it's meant to be used with 
"Import from clipboard"). But you will see that a big part of the chunk is exactly the same kind of
data structure that you need to generate as part of a Luau preset.

### Reusable Luau modules

**File extension:** `.luau` or `.lua` (without `.preset`)

The workspace folder can contain Luau files that are not presets. ReaLearn ignores them when scanning for
presets. But you can make use of them to make preset development more convenient.

Let's assume you have a function that you need in more than one preset. Instead of pasting it into each preset, you can
put that function into a module and require it in each preset. Luau presets can use the function `require()` to include 
Luau modules that are located in the same workspace. The path passed to `require()` is always relative to the workspace
root folder.

**Example:** `local my_module = require("my_module")` makes the contents of module `my_module.luau` available in your
preset.

When ReaLearn creates a workspace for you, it will put a few useful modules into your workspace root. You are free
to use them for developing your own presets. The most important file is `realearn.lua`, which contains Luau type
declarations and helper functions. Have a look at the factory presets to see how to make use of them.

Having said all that ... you don't need to use any type declarations, annotations or helper functions, they are 
completely optional. As long as the value that your script returns is a table and all the entries in that table 
meet ReaLearn's expectations, you are good.

## Basic process of coding presets

The basic process of coding presets goes like this:

1. Open the preset in your text editor
2. Make modifications
3. In ReaLearn, choose "Menu → Preset-related → Reload all presets from disk"
4. Find the preset in the preset menu and (re)load it into your compartment

If you code presets that reside in your *user workspace* (the folder that has the same name as your operating-system 
username), a welcome shortcut is available:

1. Open the preset in your text editor
2. Make modifications
3. Copy the complete code to the clipboard
4. In ReaLearn, press "Import from clipboard"

This doesn't "officially" load the preset but imports its contents, which has almost the same effect. Please note that
ReaLearn can't detect from where you copied the code. That's why if the copied Luau code uses `require()`, ReaLearn 
will look for the required module **in the user workspace only**.


## Development environment

You can use *any* text editor to code ReaLearn presets. For basic auto-completion and other goodies, it should
have special support for the [Luau](https://luau-lang.org/) language.

For a good coding experience right out of the box, I can recommend [Visual Studio Code](https://code.visualstudio.com/). 
The workspace created by ReaLearn is pre-configured to play nice with it:

1. Open Visual Studio Code
2. File → Open Folder... → Choose the workspace directory (in which this `README.md` file is located) and press "Open"
3. View → Command Palette... → Search for "Extension: Show Recommended Extensions"
4. Install the recommended extensions

Now you are good to go! Open one of the factory presets and enjoy all syntax highlighting, code completion, 
code_formatting, etc. :)

