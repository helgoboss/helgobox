local columns = {}

for i = 2, 9 do
    local track_guid = realearn.get_track_guid_by_index(i - 1);
    -- realearn.print(track_guid)
    if not track_guid then
        break
    end
    local column = {
        clip_play_settings = {
            mode = {
                kind = "ExclusiveFollowingScene",
            },
            audio_settings = {},
            track = track_guid,
        },
        clip_record_settings = {
            origin = {
                kind = "TrackInput",
            },
        },
        slots = nil
    }
    table.insert(columns, column)
end

return {
    kind = "ClipMatrix",
    value = {
        columns = columns,
        rows = {
            {},
            {},
            {},
            {},
            {},
            {},
            {},
            {},
        },
        clip_play_settings = {
            start_timing = {
                kind = "Quantized",
                numerator = 1,
                denominator = 1,
            },
            stop_timing = {
                kind = "LikeClipStartTiming",
            },
            audio_settings = {
                resample_mode = {
                    kind = "ProjectDefault",
                },
                time_stretch_mode = {
                    kind = "VariSpeed",
                },
                cache_behavior = {
                    kind = "DirectFromDisk",
                },
            },
        },
        clip_record_settings = {
            start_timing = {
                kind = "LikeClipPlayStartTiming",
            },
            stop_timing = {
                kind = "LikeClipRecordStartTiming",
            },
            duration = {
                kind = "Quantized",
                numerator = 1,
                denominator = 1,
            },
            play_start_timing = {
                kind = "Inherit",
            },
            play_stop_timing = {
                kind = "Inherit",
            },
            time_base = {
                kind = "DeriveFromRecordTiming",
            },
            looped = true,
            lead_tempo = false,
            midi_settings = {
                record_mode = {
                    kind = "Overdub",
                },
                detect_downbeat = true,
                detect_input = false,
                auto_quantize = false,
            },
            audio_settings = {
                detect_downbeat = false,
                detect_input = false,
            },
        },
        common_tempo_range = {
            min = 80,
            max = 200,
        },
    },
}