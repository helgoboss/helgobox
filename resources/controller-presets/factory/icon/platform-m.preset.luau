--- name: iCON Platform M+
--- realearn_version: 2.16.0
--- author: helgoboss
--- device_manufacturer: iCON
--- device_name: Platform M+
--- description: |
---     This controller preset implements support for the iCON Platform M+ controller in MCU mode.
---     
---     Tested with firmware v2.12.
--- setup_instructions: |
---     Please start the device in "Mackie" control mode (channel 1 since firmware version 2).
---
---     "Mackie" mode is preferred over "User defined" mode, even if "User defined" by default 
---     transmits the same messages and therefore also just works. The significant difference is that "Mackie"
---     mode supports the complete 10-bit fader resolution (1024 different values) whereas "User defined" mode
---     uses only 7 bits (128 different values). It’s a complete mystery to me why they don’t use the full resolution
---     in "User defined" mode. But that’s no problem, just make sure you use "Mackie" mode. It provides the best
---     settings out of the box. Leave the customization to ReaLearn!
--- provided_schemes: [daw]

--!strict

local commons = require("mackie/control-universal-lib/preset-common")

local companion_data = {
    controls = {
        {
            height = 32,
            id = "07226df9-370c-4495-8b58-172a3a50d7e6",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch1/v-pot/control",
                "ch1/v-select",
            },
            shape = "circle",
            width = 32,
            x = 0,
            y = 0,
        },
        {
            height = 32,
            id = "b718c836-0718-46a1-860d-9bf8c75c8018",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch2/v-pot/control",
                "ch2/v-select",
            },
            shape = "circle",
            width = 32,
            x = 128,
            y = 0,
        },
        {
            height = 32,
            id = "37ea8015-c89f-4d82-9ab9-7c9b81eb29ae",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch3/v-pot/control",
                "ch3/v-select",
            },
            shape = "circle",
            width = 32,
            x = 256,
            y = 0,
        },
        {
            height = 32,
            id = "1fa0bba0-80e7-4de3-b4ed-63ff6b3c2410",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch4/v-pot/control",
                "ch4/v-select",
            },
            shape = "circle",
            width = 32,
            x = 384,
            y = 0,
        },
        {
            height = 32,
            id = "7beac804-921e-4437-8a78-533427714619",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch5/v-pot/control",
                "ch5/v-select",
            },
            shape = "circle",
            width = 32,
            x = 512,
            y = 0,
        },
        {
            height = 32,
            id = "884daa2b-78cd-4f04-99c2-813fa21173ce",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch6/v-pot/control",
                "ch6/v-select",
            },
            shape = "circle",
            width = 32,
            x = 640,
            y = 0,
        },
        {
            height = 32,
            id = "f00ba6a6-ded6-4757-aee5-59098d475d0e",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch7/v-pot/control",
                "ch7/v-select",
            },
            shape = "circle",
            width = 32,
            x = 768,
            y = 0,
        },
        {
            height = 32,
            id = "4b76aaff-67c7-4e2d-a536-dad3642ee61f",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch8/v-pot/control",
                "ch8/v-select",
            },
            shape = "circle",
            width = 32,
            x = 896,
            y = 0,
        },
        {
            height = 480,
            id = "468dc034-8e4b-470b-9a51-2646906bb6f7",
            labelOne = {
                angle = 270,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 270,
                position = "rightOfRight",
                sizeConstrained = true,
            },
            mappings = {
                "ch1/fader",
                "ch1/fader/touch",
            },
            shape = "rectangle",
            width = 32,
            x = 0,
            y = 64,
        },
        {
            height = 480,
            id = "85b2477e-1ed1-45a6-b6b0-f5d6396a84ee",
            labelOne = {
                angle = 270,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 270,
                position = "rightOfRight",
                sizeConstrained = true,
            },
            mappings = {
                "ch2/fader",
                "ch2/fader/touch",
            },
            shape = "rectangle",
            width = 32,
            x = 128,
            y = 64,
        },
        {
            height = 480,
            id = "9cda0154-357d-4bf1-9ddc-64f97909e068",
            labelOne = {
                angle = 270,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 270,
                position = "rightOfRight",
                sizeConstrained = true,
            },
            mappings = {
                "ch3/fader",
                "ch3/fader/touch",
            },
            shape = "rectangle",
            width = 32,
            x = 256,
            y = 64,
        },
        {
            height = 480,
            id = "bdddbfc4-3a80-4397-bc43-a6eb92eb60c1",
            labelOne = {
                angle = 270,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 270,
                position = "rightOfRight",
                sizeConstrained = true,
            },
            mappings = {
                "ch4/fader",
                "ch4/fader/touch",
            },
            shape = "rectangle",
            width = 32,
            x = 384,
            y = 64,
        },
        {
            height = 480,
            id = "9fe0adef-8ae9-4a07-a8f7-11923bb2c42a",
            labelOne = {
                angle = 270,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 270,
                position = "rightOfRight",
                sizeConstrained = true,
            },
            mappings = {
                "ch5/fader",
                "ch5/fader/touch",
            },
            shape = "rectangle",
            width = 32,
            x = 512,
            y = 64,
        },
        {
            height = 480,
            id = "9d1fd347-e373-4af2-81e4-ada7e80f0f68",
            labelOne = {
                angle = 270,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 270,
                position = "rightOfRight",
                sizeConstrained = true,
            },
            mappings = {
                "ch6/fader",
                "ch6/fader/touch",
            },
            shape = "rectangle",
            width = 32,
            x = 640,
            y = 64,
        },
        {
            height = 480,
            id = "259f03c6-afe4-4a5d-9e12-66f6d6032627",
            labelOne = {
                angle = 270,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 270,
                position = "rightOfRight",
                sizeConstrained = true,
            },
            mappings = {
                "ch7/fader",
                "ch7/fader/touch",
            },
            shape = "rectangle",
            width = 32,
            x = 768,
            y = 64,
        },
        {
            height = 480,
            id = "a186f1a1-e78b-4320-b99d-22255dad0ae3",
            labelOne = {
                angle = 270,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 270,
                position = "rightOfRight",
                sizeConstrained = true,
            },
            mappings = {
                "ch8/fader",
                "ch8/fader/touch",
            },
            shape = "rectangle",
            width = 32,
            x = 896,
            y = 64,
        },
        {
            height = 32,
            id = "fe42b353-b491-4a1e-8438-b1cf31d3031f",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch1/select",
            },
            shape = "rectangle",
            width = 32,
            x = 64,
            y = 288,
        },
        {
            height = 32,
            id = "fb2ced97-49e3-4ea2-8d13-1861ed41d32a",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch2/select",
            },
            shape = "rectangle",
            width = 32,
            x = 192,
            y = 288,
        },
        {
            height = 32,
            id = "2305bc12-99f0-4760-b6e5-68d04b779607",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch3/select",
            },
            shape = "rectangle",
            width = 32,
            x = 320,
            y = 288,
        },
        {
            height = 32,
            id = "053cb45b-901f-4bdb-af26-721cf50b4747",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch4/select",
            },
            shape = "rectangle",
            width = 32,
            x = 448,
            y = 288,
        },
        {
            height = 32,
            id = "5422a6c5-4d4e-4662-8f62-43dcc222150e",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch5/select",
            },
            shape = "rectangle",
            width = 32,
            x = 576,
            y = 288,
        },
        {
            height = 32,
            id = "e0c5bdac-5347-4c85-b332-f3929fc7d81a",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch6/select",
            },
            shape = "rectangle",
            width = 32,
            x = 704,
            y = 288,
        },
        {
            height = 32,
            id = "425d54e3-4962-4785-a580-78370e5eaf25",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch7/select",
            },
            shape = "rectangle",
            width = 32,
            x = 832,
            y = 288,
        },
        {
            height = 32,
            id = "f92d9b83-7ca8-4f5a-8aed-79d1f3d82e29",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch8/select",
            },
            shape = "rectangle",
            width = 32,
            x = 960,
            y = 288,
        },
        {
            height = 32,
            id = "eb1d2f34-78b3-438a-af08-c4a0c49ffcac",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch1/mute",
            },
            shape = "rectangle",
            width = 32,
            x = 64,
            y = 352,
        },
        {
            height = 32,
            id = "9a7eb5f1-c58f-42cd-a785-19e8d5a96a90",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch2/mute",
            },
            shape = "rectangle",
            width = 32,
            x = 192,
            y = 352,
        },
        {
            height = 32,
            id = "35c77ce3-664c-4266-9f72-aed38b2f8bb9",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch3/mute",
            },
            shape = "rectangle",
            width = 32,
            x = 320,
            y = 352,
        },
        {
            height = 32,
            id = "85dd9794-445f-4f46-be66-8637cec6ceee",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch4/mute",
            },
            shape = "rectangle",
            width = 32,
            x = 448,
            y = 352,
        },
        {
            height = 32,
            id = "56cde00c-718b-4947-8465-a606cbfe1a28",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch5/mute",
            },
            shape = "rectangle",
            width = 32,
            x = 576,
            y = 352,
        },
        {
            height = 32,
            id = "68ad5f8f-74d9-4872-af49-aa9bffcf01df",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch6/mute",
            },
            shape = "rectangle",
            width = 32,
            x = 704,
            y = 352,
        },
        {
            height = 32,
            id = "0181ec93-43da-4311-a13d-8fcdd5fbab1e",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch7/mute",
            },
            shape = "rectangle",
            width = 32,
            x = 832,
            y = 352,
        },
        {
            height = 32,
            id = "9a0ce8a7-5017-41b4-97c6-79b3c8bb2eae",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch8/mute",
            },
            shape = "rectangle",
            width = 32,
            x = 960,
            y = 352,
        },
        {
            height = 32,
            id = "0ff5aee5-7793-4c7f-a204-10d58ef759a2",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch1/solo",
            },
            shape = "rectangle",
            width = 32,
            x = 64,
            y = 416,
        },
        {
            height = 32,
            id = "3c4c2f4a-5e81-485b-a25a-302bd9df0a2c",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch2/solo",
            },
            shape = "rectangle",
            width = 32,
            x = 192,
            y = 416,
        },
        {
            height = 32,
            id = "991df60d-6d72-42c0-acc1-89652572afd2",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch3/solo",
            },
            shape = "rectangle",
            width = 32,
            x = 320,
            y = 416,
        },
        {
            height = 32,
            id = "9b200df7-a411-4c41-85ad-f6acc1a49d4e",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch4/solo",
            },
            shape = "rectangle",
            width = 32,
            x = 448,
            y = 416,
        },
        {
            height = 32,
            id = "50a27c62-5dd3-463f-b2fa-0cc9c7514c2f",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch5/solo",
            },
            shape = "rectangle",
            width = 32,
            x = 576,
            y = 416,
        },
        {
            height = 32,
            id = "16a1bf1a-e538-489f-84ce-2c55d89098bb",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch6/solo",
            },
            shape = "rectangle",
            width = 32,
            x = 704,
            y = 416,
        },
        {
            height = 32,
            id = "92b6b927-e5b4-454a-af6d-7d6ea616ba82",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch7/solo",
            },
            shape = "rectangle",
            width = 32,
            x = 832,
            y = 416,
        },
        {
            height = 32,
            id = "d7cc71ab-d73f-4aef-a644-7dfca7278a2e",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch8/solo",
            },
            shape = "rectangle",
            width = 32,
            x = 960,
            y = 416,
        },
        {
            height = 32,
            id = "ddc518d6-ad09-485e-ad17-2305223011cd",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch1/record-ready",
            },
            shape = "circle",
            width = 32,
            x = 64,
            y = 480,
        },
        {
            height = 32,
            id = "feb760d1-598d-4317-8f55-d1ec213eda02",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch2/record-ready",
            },
            shape = "circle",
            width = 32,
            x = 192,
            y = 480,
        },
        {
            height = 32,
            id = "76ac7ba8-7987-4e89-9ff2-48da5e88d957",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch3/record-ready",
            },
            shape = "circle",
            width = 32,
            x = 320,
            y = 480,
        },
        {
            height = 32,
            id = "a5bcfe29-b5f9-4936-8a82-3a30ef375d76",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch4/record-ready",
            },
            shape = "circle",
            width = 32,
            x = 448,
            y = 480,
        },
        {
            height = 32,
            id = "94a388fd-d187-49dc-913b-d05a7e7c9054",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch5/record-ready",
            },
            shape = "circle",
            width = 32,
            x = 576,
            y = 480,
        },
        {
            height = 32,
            id = "c4514b7c-ce78-44ca-a1f0-e047f3529223",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch6/record-ready",
            },
            shape = "circle",
            width = 32,
            x = 704,
            y = 480,
        },
        {
            height = 32,
            id = "4ee345b9-06f7-4127-a8a0-8fdfd5839e80",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch7/record-ready",
            },
            shape = "circle",
            width = 32,
            x = 832,
            y = 480,
        },
        {
            height = 32,
            id = "79d53c50-652c-4074-8446-40fbc0dc0c05",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch8/record-ready",
            },
            shape = "circle",
            width = 32,
            x = 960,
            y = 480,
        },
        {
            height = 480,
            id = "d44fcac3-7dc1-48f7-b2c9-6e6bb2d4325f",
            labelOne = {
                angle = 270,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 270,
                position = "rightOfRight",
                sizeConstrained = true,
            },
            mappings = {
                "main/fader",
                "main/fader/touch",
            },
            shape = "rectangle",
            width = 32,
            x = 1024,
            y = 64,
        },
        {
            height = 32,
            id = "861c5a09-f033-49db-b913-b479bee77aee",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "marker",
            },
            shape = "rectangle",
            width = 32,
            x = 1088,
            y = 288,
        },
        {
            height = 32,
            id = "63bc9c6f-e564-48c2-a3a1-9f4943f74aa2",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch-right",
            },
            shape = "rectangle",
            width = 32,
            x = 1216,
            y = 64,
        },
        {
            height = 32,
            id = "dccbdaff-736e-42f3-8ded-96cd83a79ef8",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "read",
            },
            shape = "rectangle",
            width = 32,
            x = 1088,
            y = 352,
        },
        {
            height = 32,
            id = "6bd70d81-77c7-4a8d-9327-36bfb50b6d3e",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "write",
            },
            shape = "rectangle",
            width = 32,
            x = 1088,
            y = 416,
        },
        {
            height = 32,
            id = "439a498a-baea-4b2e-b105-c5102dad2717",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "ch-left",
            },
            shape = "rectangle",
            width = 32,
            x = 1152,
            y = 64,
        },
        {
            height = 32,
            id = "1fd6fd2f-7951-46db-b223-4eeb018a95bb",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "bank-left",
            },
            shape = "rectangle",
            width = 32,
            x = 1152,
            y = 128,
        },
        {
            height = 32,
            id = "5a5ab1d2-d3fd-4f0f-a479-4167ae7f7527",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "bank-right",
            },
            shape = "rectangle",
            width = 32,
            x = 1216,
            y = 128,
        },
        {
            height = 32,
            id = "35ffd77c-f379-4a4e-b5c1-0f8a5ae46a18",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "rewind",
            },
            shape = "rectangle",
            width = 32,
            x = 1152,
            y = 224,
        },
        {
            height = 32,
            id = "40313ac9-41dc-40d0-a95d-5df07fe57c9a",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "fast-fwd",
            },
            shape = "rectangle",
            width = 32,
            x = 1216,
            y = 224,
        },
        {
            height = 32,
            id = "9a85c51b-8a74-4c06-b4ba-7dfa5f126a5d",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "play",
            },
            shape = "rectangle",
            width = 32,
            x = 1152,
            y = 288,
        },
        {
            height = 32,
            id = "377c7fff-bcc6-4c4b-8efe-70795f110043",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "stop",
            },
            shape = "rectangle",
            width = 32,
            x = 1216,
            y = 288,
        },
        {
            height = 32,
            id = "ebae16ac-5eaf-4919-ab6e-bf3aa667aaf8",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "record",
            },
            shape = "rectangle",
            width = 32,
            x = 1152,
            y = 352,
        },
        {
            height = 32,
            id = "df79aada-345e-4458-9c46-4744cc1a2f06",
            labelOne = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "cycle",
            },
            shape = "rectangle",
            width = 32,
            x = 1216,
            y = 352,
        },
        {
            height = 32,
            id = "d9daf5d8-8384-4ea5-8f23-a6b528a78461",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "belowBottom",
                sizeConstrained = true,
            },
            mappings = {
                "zoom",
            },
            shape = "rectangle",
            width = 96,
            x = 1152,
            y = 416,
        },
        {
            height = 32,
            id = "903f3a94-5d67-4a9d-94ca-44ef67257aa6",
            labelOne = {
                angle = 0,
                position = "aboveTop",
                sizeConstrained = true,
            },
            labelTwo = {
                angle = 0,
                position = "center",
                sizeConstrained = true,
            },
            mappings = {
                "jog",
                "scrub",
            },
            shape = "circle",
            width = 96,
            x = 1152,
            y = 480,
        },
    },
    gridDivisionCount = 2,
    gridSize = 32,
}

return commons.create_compartment {
    companion_data = companion_data,
    support_x_touch_colors = false,
}