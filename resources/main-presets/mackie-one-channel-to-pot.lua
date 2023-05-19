return {
    kind = "MainCompartment",
    version = "2.15.0-pre.3",
    value = {
        parameters = {
            {
                index = 0,
                name = "Filter bank",
                value_count = 10,
            },
        },
        groups = {
            {
                id = "RZQLaoQOQwZFBCRaH8kHB",
                name = "Filter pages",
            },
            {
                id = "tRUhYa0RGcup1XIyV3zvr",
                name = "Preset browsing",
            },
            {
                id = "C_6AY80tdoM1Cy9FHi7XN",
                name = "Filter page \"Character\"",
                activation_condition = {
                    kind = "Bank",
                    parameter = 0,
                    bank_index = 4,
                },
            },
            {
                id = "EFpF3FW5Lp6k6jKBSh6_I",
                name = "Filter page \"Sub type\"",
                activation_condition = {
                    kind = "Bank",
                    parameter = 0,
                    bank_index = 3,
                },
            },
            {
                id = "lpnoZyrYUdSappcySl38d",
                name = "Filter page \"Type\"",
                activation_condition = {
                    kind = "Bank",
                    parameter = 0,
                    bank_index = 2,
                },
            },
            {
                id = "5A6qajMq6KFFVs8sVEIGV",
                name = "Filter page \"Bank\"",
                activation_condition = {
                    kind = "Bank",
                    parameter = 0,
                    bank_index = 1,
                },
            },
            {
                id = "zbTTUwkU-D-xslTQjGa3m",
                name = "Filter page \"Instrument\"",
                activation_condition = {
                    kind = "Bank",
                    parameter = 0,
                    bank_index = 0,
                },
            },
        },
        mappings = {
            {
                id = "MaMuToR-Qp8dB1it-ESP_",
                name = "Choose instrument",
                group = "RZQLaoQOQwZFBCRaH8kHB",
                source = {
                    kind = "Virtual",
                    id = "f1",
                    character = "Button",
                },
                glue = {
                    target_interval = {0, 0},
                    out_of_range_behavior = "Min",
                    step_size_interval = {0.01, 0.05},
                    step_factor_interval = {1, 5},
                    button_filter = "PressOnly",
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
                id = "7sIZfWbiSjYGqZR8ebE3m",
                name = "Choose bank",
                group = "RZQLaoQOQwZFBCRaH8kHB",
                source = {
                    kind = "Virtual",
                    id = "f2",
                    character = "Button",
                },
                glue = {
                    target_interval = {0.1111111111111111, 0.1111111111111111},
                    out_of_range_behavior = "Min",
                    step_size_interval = {0.01, 0.05},
                    step_factor_interval = {1, 5},
                    button_filter = "PressOnly",
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
                id = "1ZSzf-B4xTurAkVFkss3y",
                name = "Choose type",
                group = "RZQLaoQOQwZFBCRaH8kHB",
                source = {
                    kind = "Virtual",
                    id = "f3",
                    character = "Button",
                },
                glue = {
                    target_interval = {0.2222222222222222, 0.2222222222222222},
                    out_of_range_behavior = "Min",
                    step_size_interval = {0.01, 0.05},
                    step_factor_interval = {1, 5},
                    button_filter = "PressOnly",
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
                id = "t3jwdnxgLSgNwnYhfV3bX",
                name = "Choose sub type",
                group = "RZQLaoQOQwZFBCRaH8kHB",
                source = {
                    kind = "Virtual",
                    id = "f4",
                    character = "Button",
                },
                glue = {
                    target_interval = {0.3333333333333333, 0.3333333333333333},
                    out_of_range_behavior = "Min",
                    step_size_interval = {0.01, 0.05},
                    step_factor_interval = {1, 5},
                    button_filter = "PressOnly",
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
                id = "XrMjoK-jksKjBvkJR560T",
                name = "Choose character",
                group = "RZQLaoQOQwZFBCRaH8kHB",
                source = {
                    kind = "Virtual",
                    id = "f5",
                    character = "Button",
                },
                glue = {
                    target_interval = {0.4444444444444444, 0.4444444444444444},
                    out_of_range_behavior = "Min",
                    step_size_interval = {0.01, 0.05},
                    step_factor_interval = {1, 5},
                    button_filter = "PressOnly",
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
                id = "GW6k2reFw5uSNWg2gFGqS",
                name = "Browse",
                group = "zbTTUwkU-D-xslTQjGa3m",
                feedback_enabled = false,
                source = {
                    kind = "Virtual",
                    id = "jog",
                },
                glue = {
                    step_size_interval = {0.000041237113402061855, 0.000041237113402061855},
                },
                target = {
                    kind = "BrowsePotFilterItems",
                    item_kind = "Bank",
                },
            },
            {
                id = "nVTUaxgroOo_YfugMyk0X",
                name = "Browse",
                group = "5A6qajMq6KFFVs8sVEIGV",
                feedback_enabled = false,
                source = {
                    kind = "Virtual",
                    id = "jog",
                },
                glue = {
                    step_size_interval = {0.000041237113402061855, 0.000041237113402061855},
                },
                target = {
                    kind = "BrowsePotFilterItems",
                    item_kind = "SubBank",
                },
            },
            {
                id = "LryQpIKLKRMLwHA6NfDTB",
                name = "Browse",
                group = "lpnoZyrYUdSappcySl38d",
                feedback_enabled = false,
                source = {
                    kind = "Virtual",
                    id = "jog",
                },
                glue = {
                    step_size_interval = {0.000041237113402061855, 0.000041237113402061855},
                },
                target = {
                    kind = "BrowsePotFilterItems",
                    item_kind = "Category",
                },
            },
            {
                id = "rKGURMBGDTLps-HDQGbz-",
                name = "Browse",
                group = "EFpF3FW5Lp6k6jKBSh6_I",
                feedback_enabled = false,
                source = {
                    kind = "Virtual",
                    id = "jog",
                },
                glue = {
                    step_size_interval = {0.000041237113402061855, 0.000041237113402061855},
                },
                target = {
                    kind = "BrowsePotFilterItems",
                    item_kind = "SubCategory",
                },
            },
            {
                id = "BWAF5SbeC58ABUfgs15Ul",
                name = "Browse",
                group = "C_6AY80tdoM1Cy9FHi7XN",
                feedback_enabled = false,
                source = {
                    kind = "Virtual",
                    id = "jog",
                },
                glue = {
                    step_size_interval = {0.000041237113402061855, 0.000041237113402061855},
                },
                target = {
                    kind = "BrowsePotFilterItems",
                    item_kind = "Mode",
                },
            },
            {
                id = "d0avz2uKc1Kc3jot4FJF7",
                name = "Display",
                group = "C_6AY80tdoM1Cy9FHi7XN",
                control_enabled = false,
                source = {
                    kind = "MackieSevenSegmentDisplay",
                    scope = "Tc",
                },
                glue = {
                    absolute_mode = "IncrementalButton",
                    step_size_interval = {0.000041237113402061855, 0.000041237113402061855},
                    feedback = {
                        kind = "Text",
                    },
                },
                target = {
                    kind = "BrowsePotFilterItems",
                    item_kind = "Mode",
                },
            },
            {
                id = "PG926UQqFa1tKbX7klaq1",
                name = "Browse presets coarse",
                group = "tRUhYa0RGcup1XIyV3zvr",
                source = {
                    kind = "Virtual",
                    id = "ch1/fader",
                },
                glue = {
                    step_size_interval = {0.000041237113402061855, 0.000041237113402061855},
                },
                target = {
                    kind = "BrowsePotPresets",
                },
            },
            {
                id = "OpDDrpDG2hAv5gDjYubfk",
                name = "Browse presets fine",
                group = "tRUhYa0RGcup1XIyV3zvr",
                source = {
                    kind = "Virtual",
                    id = "ch1/v-pot",
                },
                glue = {
                    step_size_interval = {0.000041237113402061855, 0.000041237113402061855},
                },
                target = {
                    kind = "BrowsePotPresets",
                },
            },
            {
                id = "iw_ng6_2JADZESl3X2axi",
                name = "Previous preset",
                group = "tRUhYa0RGcup1XIyV3zvr",
                feedback_enabled = false,
                source = {
                    kind = "Virtual",
                    id = "cursor-left",
                    character = "Button",
                },
                glue = {
                    absolute_mode = "IncrementalButton",
                    reverse = true,
                    step_size_interval = {0.000041237113402061855, 0.000041237113402061855},
                },
                target = {
                    kind = "BrowsePotPresets",
                },
            },
            {
                id = "6qovylkC3n0J9DqkeIhTO",
                name = "Next preset",
                group = "tRUhYa0RGcup1XIyV3zvr",
                feedback_enabled = false,
                source = {
                    kind = "Virtual",
                    id = "cursor-right",
                    character = "Button",
                },
                glue = {
                    absolute_mode = "IncrementalButton",
                    step_size_interval = {0.000041237113402061855, 0.000041237113402061855},
                },
                target = {
                    kind = "BrowsePotPresets",
                },
            },
            {
                id = "RJDETxxijd4tIv36zzv51",
                name = "Display preset name",
                group = "tRUhYa0RGcup1XIyV3zvr",
                control_enabled = false,
                source = {
                    kind = "MackieLcd",
                    channel = 0,
                },
                glue = {
                    absolute_mode = "IncrementalButton",
                    step_size_interval = {0.000041237113402061855, 0.000041237113402061855},
                    feedback = {
                        kind = "Text",
                    },
                },
                target = {
                    kind = "BrowsePotPresets",
                },
            },
            {
                id = "9XuuQ0Tl315m561HPwdz1",
                name = "Say preset name",
                group = "tRUhYa0RGcup1XIyV3zvr",
                enabled = false,
                control_enabled = false,
                source = {
                    kind = "MidiDeviceChanges",
                },
                glue = {
                    absolute_mode = "IncrementalButton",
                    step_size_interval = {0.000041237113402061855, 0.000041237113402061855},
                    feedback = {
                        kind = "Text",
                    },
                },
                target = {
                    kind = "BrowsePotPresets",
                },
            },
            {
                id = "Kw572-DOMjW2-lAX-lTks",
                name = "Preview preset",
                group = "tRUhYa0RGcup1XIyV3zvr",
                feedback_enabled = false,
                source = {
                    kind = "Virtual",
                    id = "play",
                    character = "Button",
                },
                glue = {
                    step_size_interval = {0.01, 0.05},
                    feedback = {
                        kind = "Text",
                    },
                },
                target = {
                    kind = "PreviewPotPreset",
                },
            },
            {
                id = "a_OaKe1Uo_FATkupnHi1z",
                name = "Load preset",
                group = "tRUhYa0RGcup1XIyV3zvr",
                feedback_enabled = false,
                source = {
                    kind = "Virtual",
                    id = "ch1/v-select",
                    character = "Button",
                },
                glue = {
                    step_size_interval = {0.01, 0.05},
                    feedback = {
                        kind = "Text",
                    },
                },
                target = {
                    kind = "LoadPotPreset",
                    fx = {
                        address = "ByIndex",
                        chain = {
                            address = "Track",
                            track = {
                                address = "ById",
                                id = "B63E9F43-C847-FA42-A5DD-EAE998F42C78",
                            },
                        },
                        index = 1,
                    },
                },
            },
            {
                id = "IEVDSWvucmp0a60j0NSeG",
                name = "Show FX on load",
                group = "tRUhYa0RGcup1XIyV3zvr",
                feedback_enabled = false,
                source = {
                    kind = "Virtual",
                    id = "ch1/v-select",
                    character = "Button",
                },
                glue = {
                    target_interval = {1, 1},
                    step_size_interval = {0.01, 0.05},
                    feedback = {
                        kind = "Text",
                    },
                },
                target = {
                    kind = "FxVisibility",
                    fx = {
                        address = "ByIndex",
                        chain = {
                            address = "Track",
                            track = {
                                address = "ById",
                                id = "B63E9F43-C847-FA42-A5DD-EAE998F42C78",
                            },
                        },
                        index = 1,
                    },
                },
            },
            {
                id = "kuUh62ZJXpQa9AS_s9WvM",
                name = "Display param name",
                group = "tRUhYa0RGcup1XIyV3zvr",
                source = {
                    kind = "MackieLcd",
                    channel = 0,
                    line = 0,
                },
                glue = {
                    step_size_interval = {0.01, 0.05},
                    step_factor_interval = {1, 5},
                    feedback = {
                        kind = "Text",
                        text_expression = "{{ target.fx_parameter.name }}",
                    },
                },
                target = {
                    kind = "LastTouched",
                    included_targets = {
                        "RoutePan",
                        "TrackMuteState",
                        "TrackAutomationMode",
                        "TrackArmState",
                        "TrackVolume",
                        "TrackSoloState",
                        "TrackMonitoringMode",
                        "FxOnOffState",
                        "TrackPan",
                        "FxParameterValue",
                        "AutomationModeOverride",
                        "RouteVolume",
                        "BrowseFxPresets",
                        "PlayRate",
                        "TrackSelectionState",
                        "Tempo",
                    },
                },
            },
            {
                id = "VJjcCHS4L9XXAEZaggw_t",
                name = "Display param value",
                group = "tRUhYa0RGcup1XIyV3zvr",
                source = {
                    kind = "MackieLcd",
                    channel = 0,
                    line = 1,
                },
                glue = {
                    step_size_interval = {0.01, 0.05},
                    step_factor_interval = {1, 5},
                    feedback = {
                        kind = "Text",
                    },
                },
                target = {
                    kind = "LastTouched",
                    included_targets = {
                        "TrackSoloState",
                        "TrackSelectionState",
                        "PlayRate",
                        "RouteVolume",
                        "TrackArmState",
                        "Tempo",
                        "TrackMuteState",
                        "TrackPan",
                        "RoutePan",
                        "AutomationModeOverride",
                        "TrackMonitoringMode",
                        "TrackVolume",
                        "FxParameterValue",
                        "TrackAutomationMode",
                        "BrowseFxPresets",
                        "FxOnOffState",
                    },
                },
            },
            {
                id = "OCsMy1ZHN-pNVtrHPHUqD",
                name = "Display",
                group = "5A6qajMq6KFFVs8sVEIGV",
                control_enabled = false,
                source = {
                    kind = "MackieSevenSegmentDisplay",
                    scope = "Tc",
                },
                glue = {
                    absolute_mode = "IncrementalButton",
                    step_size_interval = {0.000041237113402061855, 0.000041237113402061855},
                    feedback = {
                        kind = "Text",
                        text_expression = "{{ target.item.name }}",
                    },
                },
                target = {
                    kind = "BrowsePotFilterItems",
                    item_kind = "SubBank",
                },
            },
            {
                id = "YCoIZWojUdlV4HLG_rpJu",
                name = "Display",
                group = "zbTTUwkU-D-xslTQjGa3m",
                control_enabled = false,
                source = {
                    kind = "MackieSevenSegmentDisplay",
                    scope = "Tc",
                },
                glue = {
                    absolute_mode = "IncrementalButton",
                    step_size_interval = {0.000041237113402061855, 0.000041237113402061855},
                    feedback = {
                        kind = "Text",
                    },
                },
                target = {
                    kind = "BrowsePotFilterItems",
                    item_kind = "Bank",
                },
            },
            {
                id = "_NzMo9-xSF5LUmblvGLBJ",
                name = "Display",
                group = "EFpF3FW5Lp6k6jKBSh6_I",
                control_enabled = false,
                source = {
                    kind = "MackieSevenSegmentDisplay",
                    scope = "Tc",
                },
                glue = {
                    absolute_mode = "IncrementalButton",
                    step_size_interval = {0.000041237113402061855, 0.000041237113402061855},
                    feedback = {
                        kind = "Text",
                        text_expression = "{{ target.item.name }}",
                    },
                },
                target = {
                    kind = "BrowsePotFilterItems",
                    item_kind = "SubCategory",
                },
            },
            {
                id = "m9DAsw94DscrY0TO0n3Fn",
                name = "Display",
                group = "lpnoZyrYUdSappcySl38d",
                control_enabled = false,
                source = {
                    kind = "MackieSevenSegmentDisplay",
                    scope = "Tc",
                },
                glue = {
                    absolute_mode = "IncrementalButton",
                    step_size_interval = {0.000041237113402061855, 0.000041237113402061855},
                    feedback = {
                        kind = "Text",
                    },
                },
                target = {
                    kind = "BrowsePotFilterItems",
                    item_kind = "Category",
                },
            },
        },
    },
}