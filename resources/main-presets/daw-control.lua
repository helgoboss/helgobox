local channel_count = 8;

local parameters = {
    {
        index = 0,
        name = "Track",
    },
    {
        index = 1,
        name = "Seek coarse/fine",
    },
    {
        index = 2,
        name = "Zoom on/off",
    },
}

local groups = {
    {
        id = "conditional-switches",
        name = "Conditional switches",
    },
    {
        id = "volume",
        name = "Volume",
    },
    {
        id = "pan",
        name = "Pan",
    },
    {
        id = "pan-reset",
        name = "Pan reset",
    },
    {
        id = "selection",
        name = "Selection",
    },
    {
        id = "mute",
        name = "Mute",
    },
    {
        id = "solo",
        name = "Solo",
    },
    {
        id = "arm",
        name = "Arm",
    },
    {
        id = "touch",
        name = "Touch",
    },
    {
        id = "transport",
        name = "Transport",
    },
    {
        id = "zoom",
        name = "Zoom",
        activation_condition = {
            kind = "Modifier",
            modifiers = {
                {
                    parameter = 2,
                    on = true,
                },
            },
        },
    },
    {
        id = "master",
        name = "Master",
    },
    {
        id = "scroll",
        name = "Scroll",
        activation_condition = {
            kind = "Modifier",
            modifiers = {
                {
                    parameter = 2,
                    on = false,
                },
            },
        },
    },
    {
        id = "lcd",
        name = "LCD",
    },
    {
        id = "meter",
        name = "Meter",
    },
    {
        id = "time-modes",
        name = "Time modes",
    },
}

local mappings = {
    {
        id = "5cfd2ff0-85ce-4e70-98e8-eb53e5e94bb1",
        name = "Bank -",
        group = "conditional-switches",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "bank-left",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            reverse = true,
            step_size_interval = {0.0008, 0.0008},
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 0,
            },
        },
    },
    {
        id = "07b0202f-b8b8-48e4-a530-f228450864f0",
        name = "Bank - faster",
        group = "conditional-switches",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "bank-left",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            reverse = true,
            step_size_interval = {0.0008, 0.0008},
            fire_mode = {
                kind = "AfterTimeoutKeepFiring",
                timeout = 300,
                rate = 100,
            },
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 0,
            },
        },
    },
    {
        id = "0baab91f-2c4e-43ae-8cac-dca727963b46",
        name = "Bank +",
        group = "conditional-switches",
        source = {
            kind = "Virtual",
            id = "bank-right",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            step_size_interval = {0.0008, 0.0008},
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 0,
            },
        },
    },
    {
        id = "0a7e122c-3eda-4355-906a-a057eefa515b",
        name = "Bank + faster",
        group = "conditional-switches",
        source = {
            kind = "Virtual",
            id = "bank-right",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            step_size_interval = {0.0008, 0.0008},
            fire_mode = {
                kind = "AfterTimeoutKeepFiring",
                timeout = 300,
                rate = 100,
            },
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 0,
            },
        },
    },
    {
        id = "cf45689a-5537-465d-ad44-460dbebf4802",
        name = "Track -",
        group = "conditional-switches",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "ch-left",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            reverse = true,
            step_size_interval = {0.0001, 0.0001},
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 0,
            },
        },
    },
    {
        id = "76465b5f-c9b3-4aa6-ba77-b891021cf38e",
        name = "Track - faster",
        group = "conditional-switches",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "ch-left",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            reverse = true,
            step_size_interval = {0.0001, 0.0001},
            fire_mode = {
                kind = "AfterTimeoutKeepFiring",
                timeout = 300,
                rate = 60,
            },
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 0,
            },
        },
    },
    {
        id = "d40ffa44-a446-4499-b9d6-787c5ff8e188",
        name = "Track +",
        group = "conditional-switches",
        source = {
            kind = "Virtual",
            id = "ch-right",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            step_size_interval = {0.0001, 0.0001},
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 0,
            },
        },
    },
    {
        id = "97db991c-a15b-4c27-aeb2-04b2ae3242c2",
        name = "Track + faster",
        group = "conditional-switches",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "ch-right",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            step_size_interval = {0.0001, 0.0001},
            fire_mode = {
                kind = "AfterTimeoutKeepFiring",
                timeout = 300,
                rate = 60,
            },
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 0,
            },
        },
    },
    {
        id = "afa35458-3160-4f49-bb90-279aae51779f",
        name = "Play/pause",
        group = "transport",
        source = {
            kind = "Virtual",
            id = "play",
            character = "Button",
        },
        glue = {
            absolute_mode = "ToggleButton",
        },
        target = {
            kind = "TransportAction",
            action = "PlayPause",
        },
    },
    {
        id = "67b3a516-246f-43a6-b850-050eb415520a",
        name = "Stop",
        group = "transport",
        source = {
            kind = "Virtual",
            id = "stop",
            character = "Button",
        },
        glue = {
            absolute_mode = "ToggleButton",
            out_of_range_behavior = "Min",
        },
        target = {
            kind = "TransportAction",
            action = "Stop",
        },
    },
    {
        id = "f14ebd63-0127-496d-9acb-281b5a7bb3db",
        name = "Repeat",
        group = "transport",
        source = {
            kind = "Virtual",
            id = "cycle",
            character = "Button",
        },
        glue = {
            absolute_mode = "ToggleButton",
            out_of_range_behavior = "Min",
        },
        target = {
            kind = "TransportAction",
            action = "Repeat",
        },
    },
    {
        id = "00626498-80a6-41d4-bc9f-f273098e3dcf",
        name = "Record",
        group = "transport",
        source = {
            kind = "Virtual",
            id = "record",
            character = "Button",
        },
        glue = {
            absolute_mode = "ToggleButton",
            out_of_range_behavior = "Min",
        },
        target = {
            kind = "TransportAction",
            action = "Record",
        },
    },
    {
        id = "1f827b7b-db04-478a-a05b-36e898f6b245",
        name = "Scrub coarse",
        group = "transport",
        feedback_enabled = false,
        activation_condition = {
            kind = "Modifier",
            modifiers = {
                {
                    parameter = 1,
                    on = false,
                },
            },
        },
        source = {
            kind = "Virtual",
            id = "jog",
        },
        target = {
            kind = "ReaperAction",
            command = 992,
            invocation = "Relative",
        },
    },
    {
        id = "a259062e-dfde-4e6c-8e0a-014a078fc1be",
        name = "Scrub fine",
        group = "transport",
        feedback_enabled = false,
        activation_condition = {
            kind = "Modifier",
            modifiers = {
                {
                    parameter = 1,
                    on = true,
                },
            },
        },
        source = {
            kind = "Virtual",
            id = "jog",
        },
        glue = {
            step_factor_interval = {0, 0},
        },
        target = {
            kind = "ReaperAction",
            command = 974,
            invocation = "Relative",
        },
    },
    {
        id = "1e5c604f-9040-47c0-ab37-a05383e7be00",
        name = "Seek coarse/fine",
        group = "conditional-switches",
        source = {
            kind = "Virtual",
            id = "scrub",
            character = "Button",
        },
        glue = {
            absolute_mode = "ToggleButton",
            fire_mode = {
                kind = "Normal",
                press_duration_interval = {0, 250},
            },
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 1,
            },
        },
    },
    {
        id = "ee81c405-d355-45e0-80b4-913633295193",
        name = "Zoom on/off",
        group = "conditional-switches",
        source = {
            kind = "Virtual",
            id = "zoom",
            character = "Button",
        },
        glue = {
            absolute_mode = "ToggleButton",
            fire_mode = {
                kind = "Normal",
                press_duration_interval = {0, 250},
            },
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 2,
            },
        },
    },
    {
        id = "f13678b1-da81-46a5-b318-861a813d7183",
        name = "Previous",
        group = "transport",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "rewind",
            character = "Button",
        },
        target = {
            kind = "ReaperAction",
            command = 40172,
            invocation = "Trigger",
        },
    },
    {
        name = "Previous LED",
        group = "transport",
        control_enabled = false,
        source = {
            kind = "Virtual",
            id = "rewind",
            character = "Button",
        },
        glue = {
            target_interval = {0, 0.00001},
            jump_interval = {0, 0.00001},
        },
        target = {
            kind = "Seek",
        },
    },
    {
        id = "7f0d2cbb-0346-4229-89dd-db58cac3a460",
        name = "Next",
        group = "transport",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "fast-fwd",
            character = "Button",
        },
        target = {
            kind = "ReaperAction",
            command = 40173,
            invocation = "Trigger",
        },
    },
    {
        name = "Next LED",
        group = "transport",
        control_enabled = false,
        source = {
            kind = "Virtual",
            id = "fast-fwd",
            character = "Button",
        },
        glue = {
            target_interval = {0.99999, 1},
            reverse = true,
        },
        target = {
            kind = "Seek",
        },
    },
    {
        id = "8b0ce5db-a371-4b4c-9fe8-ca086bc706bf",
        name = "Zoom out horizontally",
        group = "zoom",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "cursor-left",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            reverse = true,
            fire_mode = {
                kind = "AfterTimeoutKeepFiring",
                rate = 100,
            },
        },
        target = {
            kind = "ReaperAction",
            command = 998,
            invocation = "Relative",
        },
    },
    {
        id = "9b6fe0b8-d5f3-438b-9830-6bb782121c2d",
        name = "Zoom in horizontally",
        group = "zoom",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "cursor-right",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            fire_mode = {
                kind = "AfterTimeoutKeepFiring",
                rate = 100,
            },
        },
        target = {
            kind = "ReaperAction",
            command = 998,
            invocation = "Relative",
        },
    },
    {
        id = "c3c34680-0396-4b24-8714-e210062f46cd",
        name = "Zoom out vertically",
        group = "zoom",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "cursor-down",
            character = "Button",
        },
        glue = {
            fire_mode = {
                kind = "AfterTimeoutKeepFiring",
                rate = 100,
            },
        },
        target = {
            kind = "ReaperAction",
            command = 40112,
            invocation = "Trigger",
        },
    },
    {
        id = "e93e8981-8127-446f-b5d9-34bb576fbc2b",
        name = "Zoom in vertically",
        group = "zoom",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "cursor-up",
            character = "Button",
        },
        glue = {
            fire_mode = {
                kind = "AfterTimeoutKeepFiring",
                rate = 100,
            },
        },
        target = {
            kind = "ReaperAction",
            command = 40111,
            invocation = "Trigger",
        },
    },
    {
        id = "8b6d4cf5-d4f2-4701-af71-90d839da2318",
        name = "Scroll down",
        group = "scroll",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "cursor-down",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            reverse = true,
            step_factor_interval = {8, 8},
            fire_mode = {
                kind = "AfterTimeoutKeepFiring",
                rate = 100,
            },
        },
        target = {
            kind = "ReaperAction",
            command = 989,
            invocation = "Relative",
        },
    },
    {
        id = "b633c2d1-c283-4021-a1fc-76b6abd407b9",
        name = "Scroll up",
        group = "scroll",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "cursor-up",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            step_size_interval = {0.08, 0.08},
            step_factor_interval = {8, 8},
            fire_mode = {
                kind = "AfterTimeoutKeepFiring",
                rate = 100,
            },
        },
        target = {
            kind = "ReaperAction",
            command = 989,
            invocation = "Relative",
        },
    },
    {
        id = "b9fa3db6-8bed-4c8a-82fa-580d3d0da92c",
        name = "Scroll left",
        group = "scroll",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "cursor-left",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            reverse = true,
            step_factor_interval = {8, 8},
            fire_mode = {
                kind = "AfterTimeoutKeepFiring",
                rate = 100,
            },
        },
        target = {
            kind = "ReaperAction",
            command = 40140,
            invocation = "Relative",
        },
    },
    {
        id = "344405d5-b50d-47d4-a22e-aa99a7454a25",
        name = "Scroll right",
        group = "scroll",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "cursor-right",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            reverse = true,
            step_factor_interval = {8, 8},
            fire_mode = {
                kind = "AfterTimeoutKeepFiring",
                rate = 100,
            },
        },
        target = {
            kind = "ReaperAction",
            command = 40141,
            invocation = "Relative",
        },
    },
    {
        id = "261067ef-c2f0-41de-b10f-418cc718cf1c",
        name = "Master touch",
        group = "master",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "main/fader/touch",
            character = "Button",
        },
        target = {
            kind = "TrackAutomationTouchState",
            track = {
                address = "Master",
            },
            touched_parameter = "Volume",
        },
    },
    {
        id = "2f45cdf6-bac9-4a97-8a3e-c296d2cc7e0f",
        name = "Master volume",
        group = "master",
        source = {
            kind = "Virtual",
            id = "main/fader",
        },
        target = {
            kind = "TrackVolume",
            track = {
                address = "Master",
            },
        },
    },
    {
        id = "970e04c9-e262-478e-85ce-9e84875fb5f5",
        name = "Marker",
        group = "master",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "marker",
            character = "Button",
        },
        glue = {
            out_of_range_behavior = "Min",
            button_filter = "PressOnly",
        },
        target = {
            kind = "ReaperAction",
            command = 40157,
            invocation = "Trigger",
        },
    },
    {
        id = "c49238b9-189e-42d3-984a-955fe63c46e8",
        name = "Global read",
        group = "master",
        source = {
            kind = "Virtual",
            id = "read",
            character = "Button",
        },
        glue = {
            target_interval = {1, 1},
            absolute_mode = "ToggleButton",
        },
        target = {
            kind = "AutomationModeOverride",
            override = {
                kind = "Mode",
                mode = "Read",
            },
        },
    },
    {
        id = "b805f628-16e1-42ca-ac36-3bb95133815f",
        name = "Global write",
        group = "master",
        source = {
            kind = "Virtual",
            id = "write",
            character = "Button",
        },
        glue = {
            target_interval = {1, 1},
            absolute_mode = "ToggleButton",
        },
        target = {
            kind = "AutomationModeOverride",
            override = {
                kind = "Mode",
                mode = "Touch",
            },
        },
    },
    {
        id = "13d79d42-4d59-4930-a976-b4374e0a69b6",
        name = "Click",
        group = "master",
        source = {
            kind = "Virtual",
            id = "click",
            character = "Button",
        },
        glue = {
            absolute_mode = "ToggleButton",
        },
        target = {
            kind = "ReaperAction",
            command = 40364,
        },
    },
    -- Done
    {
        id = "7b90c136-f89c-477e-b812-525b4a7da5ed",
        name = "Track LCD",
        group = "lcd",
        control_enabled = false,
        source = {
            kind = "Virtual",
            id = "lcd/assignment",
        },
        glue = {
            source_interval = {0, 1},
            target_interval = {0, 0.01},
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 0,
            },
            poll_for_feedback = false,
        },
    },
    -- Done
    {
        id = "7GFNQfZp9uqovhV5zWtvh",
        name = "Timecode",
        group = "lcd",
        control_enabled = false,
        source = {
            kind = "MackieSevenSegmentDisplay",
            scope = "Tc",
        },
        glue = {
            feedback_transformation = "{{ target.position.project_default.mcu }}",
            feedback_kind = "Text",
        },
        target = {
            kind = "Seek",
            feedback_resolution = "High",
        },
    },
    -- Time modes
    {
        name = "Measures.Beats",
        group = "time-modes",
        control_enabled = false,
        source = {
            kind = "Virtual",
            id = "smpte-beats",
            character = "Button",
        },
        glue = {
            source_interval = {0, 0},
        },
        target = {
            kind = "ReaperAction",
            command = 40411,
            invocation = "Trigger",
        },
    },
    {
        name = "Seconds",
        group = "time-modes",
        control_enabled = false,
        source = {
            kind = "Virtual",
            id = "smpte-beats",
            character = "Button",
        },
        target = {
            kind = "ReaperAction",
            command = 40412,
            invocation = "Trigger",
        },
    },
    {
        name = "Samples",
        group = "time-modes",
        control_enabled = false,
        source = {
            kind = "Virtual",
            id = "smpte-beats",
            character = "Button",
        },
        target = {
            kind = "ReaperAction",
            command = 40413,
            invocation = "Trigger",
        },
    },
    {
        name = "Hours:Minutes:Seconds:Frames",
        group = "time-modes",
        control_enabled = false,
        source = {
            kind = "Virtual",
            id = "smpte-beats",
            character = "Button",
        },
        target = {
            kind = "ReaperAction",
            command = 40414,
            invocation = "Trigger",
        },
    },
    {
        name = "Absolute Frames",
        group = "time-modes",
        control_enabled = false,
        source = {
            kind = "Virtual",
            id = "smpte-beats",
            character = "Button",
        },
        target = {
            kind = "ReaperAction",
            command = 41972,
            invocation = "Trigger",
        },
    },
    {
        name = "Minutes:Seconds",
        group = "time-modes",
        control_enabled = false,
        source = {
            kind = "Virtual",
            id = "smpte-beats",
            character = "Button",
        },
        target = {
            kind = "ReaperAction",
            command = 40410,
            invocation = "Trigger",
        },
    },
    {
        name = "Cycle time modes",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = "smpte-beats",
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            wrap = true,
        },
        target = {
            kind = "CycleThroughGroupMappings",
            group = "time-modes",
        },
    },
}

-- For each channel
for ch = 0, channel_count - 1 do
    local human_ch = ch + 1
    local prefix = "ch"..human_ch.."/"
    local track_expression = "p1 * 10000 - 1 + "..ch;
    local track_volume = {
        name = "Tr"..human_ch.." Vol",
        group = "volume",
        source = {
            kind = "Virtual",
            id = prefix.."fader",
        },
        target = {
            kind = "TrackVolume",
            track = {
                address = "Dynamic",
                expression = track_expression,
            },
        },
    }
    local track_pan_control = {
        name = "Tr"..human_ch.." Pan",
        feedback_enabled = false,
        group = "pan",
        source = {
            kind = "Virtual",
            id = prefix.."v-pot",
        },
        glue = {
            step_size_interval = {0.005, 1.0},
        },
        target = {
            kind = "TrackPan",
            track = {
                address = "Dynamic",
                expression = track_expression,
            },
        },
    }
    local track_pan_feedback = {
        name = "Tr"..human_ch.." Pan FB",
        control_enabled = false,
        group = "pan",
        source = {
            kind = "Virtual",
            id = prefix.."v-pot/boost-cut",
        },
        target = {
            kind = "TrackPan",
            track = {
                address = "Dynamic",
                expression = track_expression,
            },
        },
    }
    local track_pan_reset = {
        name = "Tr"..human_ch.." Pan Reset",
        group = "pan-reset",
        source = {
            kind = "Virtual",
            id = prefix.."v-select",
            character = "Button",
        },
        glue = {
            target_interval = {0.5, 0.5},
        },
        target = {
            kind = "TrackPan",
            track = {
                address = "Dynamic",
                expression = track_expression,
            },
        },
    }
    local track_selection = {
        name = "Tr"..human_ch.." Selection",
        group = "selection",
        source = {
            kind = "Virtual",
            id = prefix.."select",
            character = "Button",
        },
        glue = {
            absolute_mode = "ToggleButton",
        },
        target = {
            kind = "TrackSelectionState",
            track = {
                address = "Dynamic",
                expression = track_expression,
            },
            scroll_mixer = true,
        },
    }
    local track_mute = {
        name = "Tr"..human_ch.." Mute",
        group = "mute",
        source = {
            kind = "Virtual",
            id = prefix.."mute",
            character = "Button",
        },
        glue = {
            absolute_mode = "ToggleButton",
        },
        target = {
            kind = "TrackMuteState",
            track = {
                address = "Dynamic",
                expression = track_expression,
            },
        },
    }
    local track_solo = {
        name = "Tr"..human_ch.." Solo",
        group = "solo",
        source = {
            kind = "Virtual",
            id = prefix.."solo",
            character = "Button",
        },
        glue = {
            absolute_mode = "ToggleButton",
        },
        target = {
            kind = "TrackSoloState",
            track = {
                address = "Dynamic",
                expression = track_expression,
            },
        },
    }
    local track_arm = {
        name = "Tr"..human_ch.." Arm",
        group = "arm",
        source = {
            kind = "Virtual",
            id = prefix.."record-ready",
            character = "Button",
        },
        glue = {
            absolute_mode = "ToggleButton",
        },
        target = {
            kind = "TrackArmState",
            track = {
                address = "Dynamic",
                expression = track_expression,
            },
        },
    }
    local track_touch = {
        name = "Tr"..human_ch.." Touch",
        group = "touch",
        source = {
            kind = "Virtual",
            id = prefix.."fader/touch",
            character = "Button",
        },
        target = {
            kind = "TrackAutomationTouchState",
            track = {
                address = "Dynamic",
                expression = track_expression,
            },
            touched_parameter = "Volume",
        },
    }
    -- Done
    local track_name_display = {
        name = "Tr"..human_ch.." Name",
        group = "lcd",
        control_enabled = false,
        source = {
            kind = "Virtual",
            id = prefix.."lcd/line1",
        },
        glue = {
            feedback_transformation = "{{ target.track.name }}",
            feedback_kind = "Text",
        },
        target = {
            kind = "TrackVolume",
            track = {
                address = "Dynamic",
                expression = track_expression,
            },
        },
    }
    -- Done
    local track_pan_display = {
        name = "Tr"..human_ch.." Pan LCD",
        group = "lcd",
        control_enabled = false,
        source = {
            kind = "Virtual",
            id = prefix.."lcd/line2",
        },
        glue = {
            feedback_transformation = "{{ target.pan.mcu }}",
            feedback_kind = "Text",
        },
        target = {
            kind = "TrackPan",
            track = {
                address = "Dynamic",
                expression = track_expression,
            },
        },
    }
    -- Done
    local track_peak = {
        name = "Tr"..human_ch.." Peaks",
        group = "meter",
        control_enabled = false,
        source = {
            kind = "Virtual",
            id = prefix.."meter/peak",
        },
        target = {
            kind = "TrackPeak",
            track = {
                address = "Dynamic",
                expression = track_expression,
            },
        },
    }
    table.insert(mappings, track_volume)
    table.insert(mappings, track_pan_control)
    table.insert(mappings, track_pan_feedback)
    table.insert(mappings, track_pan_reset)
    table.insert(mappings, track_selection)
    table.insert(mappings, track_mute)
    table.insert(mappings, track_solo)
    table.insert(mappings, track_arm)
    table.insert(mappings, track_touch)
    table.insert(mappings, track_name_display)
    table.insert(mappings, track_pan_display)
    table.insert(mappings, track_peak)
end

return {
    kind = "MainCompartment",
    value = {
        parameters = parameters,
        groups = groups,
        mappings = mappings,
    },
}