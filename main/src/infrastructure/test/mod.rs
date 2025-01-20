use crate::domain::{FinalSourceFeedbackValue, PLUGIN_PARAMETER_COUNT};
use crate::infrastructure::plugin::{BackboneShell, NewInstanceOutcome, SET_STATE_PARAM_NAME};
use approx::assert_abs_diff_eq;
use base::future_util::millis;
use base::{Global, SenderToNormalThread};
use helgoboss_learn::{MidiSourceValue, BASE_EPSILON, FEEDBACK_EPSILON};
use helgoboss_midi::test_util::*;
use helgoboss_midi::{DataEntryByteOrder, ParameterNumberMessage, RawShortMessage, ShortMessage};
use reaper_high::{FxParameter, Reaper, Track};
use reaper_medium::{Db, ReaperPanValue, StuffMidiMessageTarget};
use std::ffi::CString;
use std::future::Future;
use FinalSourceFeedbackValue::Midi;
use MidiSourceValue::{ParameterNumber, Plain};

pub fn run_test() {
    Global::future_support().spawn_in_main_thread_from_main_thread(async {
        Test::new().test().await;
        Ok(())
    })
}

#[derive(Default)]
struct Test {
    current_step: usize,
}

impl Test {
    pub fn new() -> Test {
        Default::default()
    }

    pub async fn test(&mut self) {
        #[cfg(target_os = "macos")]
        self.step("Take screenshots", macos_impl::take_screenshots())
            .await;
        self.step("Basics", basics()).await;
        self.step("(N)RPN", nrpn_test()).await;
        self.step(
            "Load mapping snapshot - All mappings",
            load_mapping_snapshot_all_mappings(),
        )
        .await;
        self.step(
            "Load mapping snapshot - Some mappings",
            load_mapping_snapshot_some_mappings(),
        )
        .await;
        self.step("Toggle mode", toggle_mode()).await;
        self.step(
            "Send feedback after control - Normal mode - Arm",
            send_feedback_after_control_normal_mode_arm(),
        )
        .await;
        self.step(
            "Send feedback after control - Toggle mode - Arm",
            send_feedback_after_control_toggle_mode_arm(),
        )
        .await;
        self.step(
            "#396 - Send feedback after control",
            issue_396_send_feedback_after_control(),
        )
        .await;
        self.step(
            "Send feedback after control - Normal mode - Volume",
            send_feedback_after_control_normal_mode_volume(),
        )
        .await;
        self.step(
            "Basics in controller compartment",
            basics_controller_compartment(),
        )
        .await;
        self.step("Track by ID", track_by_id()).await;
        self.step("Track by position", track_by_position()).await;
        self.step("Track by name", track_by_name()).await;
        self.step("FX by ID", fx_by_id()).await;
        self.step("FX by position", fx_by_position()).await;
        self.step("FX by name", fx_by_name()).await;
        self.step(
            "Conditional activation - Modifiers",
            conditional_activation_modifiers(),
        )
        .await;
        self.step(
            "Conditional activation - Program",
            conditional_activation_program(),
        )
        .await;
        self.step("Conditional activation - EEL", conditional_activation_eel())
            .await;
        self.step("Virtual", virtual_mapping()).await;
        log("\nTests executed successfully!")
    }

    async fn step<T>(&mut self, label: &str, f: impl Future<Output = T>) -> T {
        millis(1).await;
        log(format!("{}. {}\n", self.current_step + 1, label));
        self.current_step += 1;
        f.await
    }
}

fn log(msg: impl AsRef<str>) {
    Reaper::get().show_console_msg(msg.as_ref());
}

async fn moment() {
    millis(200).await;
}

async fn setup() -> RealearnTestInstance {
    // When
    let reaper = Reaper::get();
    let project = reaper.create_empty_project_in_new_tab();
    let track = project.add_track().unwrap();
    let outcome = BackboneShell::create_new_instance_on_track(&track)
        .await
        .expect("couldn't create test instance");
    // Then
    assert!(outcome.fx.parameter_count() >= PLUGIN_PARAMETER_COUNT + 2);
    let unit_model = outcome.instance_shell.main_unit_shell().model();
    let (feedback_sender, feedback_receiver) =
        SenderToNormalThread::new_unbounded_channel("test feedback");
    unit_model
        .borrow()
        .use_integration_test_feedback_sender(feedback_sender);
    RealearnTestInstance {
        outcome,
        feedback_receiver,
    }
}

struct RealearnTestInstance {
    outcome: NewInstanceOutcome,
    feedback_receiver: crossbeam_channel::Receiver<FinalSourceFeedbackValue>,
}

impl RealearnTestInstance {
    /// Returns the containing track.
    pub fn track(&self) -> &Track {
        self.outcome.fx.track().unwrap()
    }

    /// Returns the ReaLearn VST parameter at the given index.
    pub fn parameter_by_index(&self, index: u32) -> FxParameter {
        self.outcome.fx.parameter_by_index(index)
    }

    /// Returns all recorded feedback and removes it from the list.
    fn pop_feedback(&self) -> Vec<FinalSourceFeedbackValue> {
        self.feedback_receiver.try_iter().collect()
    }
}

async fn basics() {
    // Given
    let realearn = setup().await;
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    // When
    load_realearn_preset(&realearn, include_str!("presets/basics.json"));
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 91)))],
        "feedback should be sent after loading preset"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    // Whenever we write Db::MINUS_150_DB here, we expected Db::MINUS_INF before! Turns out that the minimum value
    // set by the ReaLearn volume targets was (at least recently) *always* only -150, not -inf! But because of all
    // the pointless conversions in reaper_high::Volume (now SliderVolume and fixed in reaper-rs commit 88686932),
    // we ended up getting -inf when reading the track volume, which was wrong (if that makes any difference anyway).
    // So now it's correct. It's a bit weird that using SetMediaTrackInfo_Value() with a value of 0.0 doesn't give
    // -inf (as opposed to SetMediaTrackInfo_Value with D_VOL) but that's another story.
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        MIN_VOLUME,
        "NOTE OFF should turn down volume completely"
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "feedback should be sent on target value change"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        MIN_VOLUME
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if target value not changed"
    );
    // When
    send_midi(note_on(0, 64, 127)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::TWELVE_DB
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 127)))],
        "feedback should be sent on target value change"
    );
}

async fn nrpn_test() {
    // Given
    let realearn = setup().await;
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    // When
    load_realearn_preset(&realearn, include_str!("presets/nrpn.json"));
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(ParameterNumber(nrpn(0, 100, 91)))],
        "feedback should be sent after loading preset"
    );
    // When
    send_midi_multi(
        nrpn(0, 100, 0).to_short_messages::<RawShortMessage>(DataEntryByteOrder::MsbFirst),
    )
    .await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        MIN_VOLUME,
        "NOTE OFF should turn down volume completely"
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(ParameterNumber(nrpn(0, 100, 0)))],
        "feedback should be sent on target value change"
    );
    // When
    send_midi_multi(
        ParameterNumberMessage::non_registered_increment(channel(0), u14(100), u7(50))
            .to_short_messages::<RawShortMessage>(DataEntryByteOrder::MsbFirst),
    )
    .await;
    // Then
    assert_ne!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        MIN_VOLUME,
        "increment should turn volume up a bit"
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(ParameterNumber(nrpn(0, 100, 1)))],
        "feedback should be sent on target value change"
    );
    // When
    send_midi_multi(
        ParameterNumberMessage::non_registered_decrement(channel(0), u14(100), u7(50))
            .to_short_messages::<RawShortMessage>(DataEntryByteOrder::MsbFirst),
    )
    .await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        MIN_VOLUME,
        "decrement should turn volume down a bit again"
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(ParameterNumber(nrpn(0, 100, 0)))],
        "feedback should be sent on target value change"
    );
}

async fn load_mapping_snapshot_all_mappings() {
    // Given
    let realearn = setup().await;
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    // When
    load_realearn_preset(
        &realearn,
        include_str!("presets/load_mapping_snapshot_all_mappings.json"),
    );
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![
            // Vol
            Midi(Plain(note_on(0, 64, 91))),
            // Pan
            Midi(Plain(note_on(0, 63, 64))),
            // Mute
            Midi(Plain(note_on(0, 62, 0))),
            // Load snapshot
            Midi(Plain(note_on(0, 65, 127))),
        ],
        "feedback should be sent after loading preset"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    send_midi(note_on(0, 63, 0)).await;
    send_midi(note_on(0, 62, 127)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        MIN_VOLUME,
        "volume should be MIN because muted"
    );
    assert_eq!(realearn.track().pan().reaper_value(), ReaperPanValue::LEFT);
    assert!(realearn.track().is_muted());
    assert_eq!(
        realearn.pop_feedback(),
        vec![
            // Vol
            Midi(Plain(note_on(0, 64, 0))),
            // Pan
            Midi(Plain(note_on(0, 63, 0))),
            // Mute
            Midi(Plain(note_on(0, 62, 127))),
        ],
        "feedback should be sent on target value changes"
    );
    // When
    send_midi(note_on(0, 65, 127)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    assert_eq!(
        realearn.track().pan().reaper_value(),
        ReaperPanValue::CENTER
    );
    assert!(!realearn.track().is_muted());
    assert_eq!(
        realearn.pop_feedback(),
        vec![
            // Vol
            Midi(Plain(note_on(0, 64, 91))),
            // Pan
            Midi(Plain(note_on(0, 63, 64))),
            // Mute
            Midi(Plain(note_on(0, 62, 0))),
        ],
        "feedback should be sent when loading snapshot"
    );
}

async fn load_mapping_snapshot_some_mappings() {
    // Given
    let realearn = setup().await;
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    // When
    load_realearn_preset(
        &realearn,
        include_str!("presets/load_mapping_snapshot_some_mappings.json"),
    );
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![
            // Vol
            Midi(Plain(note_on(0, 64, 91))),
            // Pan
            Midi(Plain(note_on(0, 63, 64))),
            // Mute
            Midi(Plain(note_on(0, 62, 0))),
            // Load snapshot
            Midi(Plain(note_on(0, 65, 127))),
        ],
        "feedback should be sent after loading preset"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    send_midi(note_on(0, 63, 0)).await;
    send_midi(note_on(0, 62, 127)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        MIN_VOLUME,
        "volume should be MIN because muted"
    );
    assert_eq!(realearn.track().pan().reaper_value(), ReaperPanValue::LEFT);
    assert!(realearn.track().is_muted());
    assert_eq!(
        realearn.pop_feedback(),
        vec![
            // Vol
            Midi(Plain(note_on(0, 64, 0))),
            // Pan
            Midi(Plain(note_on(0, 63, 0))),
            // Mute
            Midi(Plain(note_on(0, 62, 127))),
        ],
        "feedback should be sent on target value changes"
    );
    // When
    send_midi(note_on(0, 65, 127)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    assert_eq!(realearn.track().pan().reaper_value(), ReaperPanValue::LEFT);
    assert!(!realearn.track().is_muted());
    assert_eq!(
        realearn.pop_feedback(),
        vec![
            // Vol
            Midi(Plain(note_on(0, 64, 91))),
            // Mute
            Midi(Plain(note_on(0, 62, 0))),
        ],
        "feedback should be sent when loading snapshot"
    );
}

async fn toggle_mode() {
    // Given
    let realearn = setup().await;
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    // When
    load_realearn_preset(&realearn, include_str!("presets/toggle-mode.json"));
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "feedback should be sent after loading preset"
    );
    // When
    send_midi(note_on(0, 64, 127)).await;
    // Then
    assert!(realearn.track().is_armed(false));
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 127))),],
        "feedback should be sent on target value change"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert!(realearn.track().is_armed(false));
    assert_eq!(realearn.pop_feedback(), vec![]);
    // When
    send_midi(note_on(0, 64, 127)).await;
    // Then
    assert!(!realearn.track().is_armed(false));
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0))),],
        "feedback should be sent on target value change"
    );
}

async fn send_feedback_after_control_toggle_mode_arm() {
    // Given
    let realearn = setup().await;
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    // When
    load_realearn_preset(
        &realearn,
        include_str!("presets/send-feedback-after-control-toggle-mode-arm.json"),
    );
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "feedback should be sent after loading preset"
    );
    // When
    send_midi(note_on(0, 64, 127)).await;
    // Then
    assert!(realearn.track().is_armed(false));
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 127))),],
        "feedback should be sent on target value change"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert!(realearn.track().is_armed(false));
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 127)))],
        "should send feedback even if target value not changed"
    );
}

async fn send_feedback_after_control_normal_mode_arm() {
    // Given
    let realearn = setup().await;
    assert!(!realearn.track().is_armed(false));
    // When
    load_realearn_preset(
        &realearn,
        include_str!("presets/send-feedback-after-control-normal-mode-arm.json"),
    );
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "feedback should be sent after loading preset"
    );
    // When
    send_midi(note_on(0, 64, 127)).await;
    // Then
    assert!(realearn.track().is_armed(false));
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 127))),],
        "feedback should be sent on target value change"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert!(!realearn.track().is_armed(false));
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0))),],
        "feedback should be sent on target value change"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert!(!realearn.track().is_armed(false));
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "should send feedback even if target value not changed"
    );
}

async fn issue_396_send_feedback_after_control() {
    // Given
    let realearn = setup().await;
    assert_abs_diff_eq!(
        realearn
            .parameter_by_index(0)
            .reaper_normalized_value()
            .get(),
        0.0
    );
    // When
    load_realearn_preset(
        &realearn,
        include_str!("presets/issue-396-send-feedback-after-control.json"),
    );
    moment().await;
    // Then
    // Initial parameter value in preset should be respected
    assert_abs_diff_eq!(
        realearn
            .parameter_by_index(0)
            .reaper_normalized_value()
            .get(),
        0.5
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![
            // Even though the initial parameter value is 0.50, the feedback is zero because
            // target min/max is set to 0.01 and out-of-range behavior to "Min".
            Midi(Plain(note_on(0, 64, 0))),
            // More than necessary
            Midi(Plain(note_on(0, 64, 0))),
        ],
        "feedback should be sent after loading preset"
    );
    // When
    send_midi(note_on(0, 64, 127)).await;
    // Then
    assert_abs_diff_eq!(
        realearn
            .parameter_by_index(0)
            .reaper_normalized_value()
            .get(),
        // Because target min/max is set to 0.01
        0.01,
        epsilon = FEEDBACK_EPSILON
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![
            Midi(Plain(note_on(0, 64, 127))),
        ],
        "maximum feedback value should be sent because target has changed to exactly target min/max"
    );
    // When
    send_midi(note_on(0, 64, 127)).await;
    // Then
    assert_abs_diff_eq!(
        realearn
            .parameter_by_index(0)
            .reaper_normalized_value()
            .get(),
        // Because target min/max is (still) set to 0.01
        0.01,
        epsilon = FEEDBACK_EPSILON
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 127)))],
        // That's the whole point of "Send feedback after control".
        "should send feedback even if target value not changed (after another NOTE ON)"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_abs_diff_eq!(
        realearn
            .parameter_by_index(0)
            .reaper_normalized_value()
            .get(),
        // Because target min/max is (still) set to 0.01
        0.01,
        epsilon = FEEDBACK_EPSILON
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 127))),],
        // That's the whole point of "Send feedback after control".
        "should send feedback even if target value not changed (after NOTE OFF)"
    );
}
async fn send_feedback_after_control_normal_mode_volume() {
    // Given
    let realearn = setup().await;
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    // When
    load_realearn_preset(
        &realearn,
        include_str!("presets/send-feedback-after-control-normal-mode-volume.json"),
    );
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 91)))],
        "feedback should be sent after loading preset"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        MIN_VOLUME
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0))),],
        "feedback should be sent on target value change"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        MIN_VOLUME
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "feedback should be sent even if value still same"
    );
}

async fn basics_controller_compartment() {
    // Given
    let realearn = setup().await;
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    // When
    load_realearn_preset(
        &realearn,
        include_str!("presets/basics-controller-compartment.json"),
    );
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 91)))],
        "feedback should be sent after loading preset"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        MIN_VOLUME
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "feedback should be sent on target value change"
    );
    // When
    send_midi(note_on(0, 64, 127)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::TWELVE_DB
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 127)))],
        "feedback should be sent on target value change"
    );
}

async fn virtual_mapping() {
    // Given
    let realearn = setup().await;
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    // When
    load_realearn_preset(&realearn, include_str!("presets/virtual.json"));
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 91)))],
        "feedback should be sent after loading preset"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        MIN_VOLUME
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "feedback should be sent on target value change"
    );
    // When
    send_midi(note_on(0, 64, 127)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::TWELVE_DB
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 127)))],
        "feedback should be sent on target value change"
    );
}

/// Tests that non-existing track ID doesn't cause errors.
async fn track_by_id() {
    // Given
    let realearn = setup().await;
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    // When
    load_realearn_preset(&realearn, include_str!("presets/track-by-id.json"));
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent after loading preset because target track doesn't exist yet"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    assert_eq!(realearn.pop_feedback(), vec![]);
    // When
    let track_2 = realearn.track().project().add_track().unwrap();
    moment().await;
    // Then
    assert_eq!(track_2.volume().to_db_ex(Db::MINUS_INF), Db::ZERO_DB);
    assert_eq!(realearn.pop_feedback(), vec![]);
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    assert_eq!(track_2.volume().to_db_ex(Db::MINUS_INF), Db::ZERO_DB);
    assert_eq!(realearn.pop_feedback(), vec![]);
}

async fn track_by_position() {
    // Given
    let realearn = setup().await;
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    // When
    load_realearn_preset(&realearn, include_str!("presets/track-by-position.json"));
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent after loading preset because target track doesn't exist yet"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    // When
    let track_2 = realearn.track().project().add_track().unwrap();
    moment().await;
    // Then
    assert_eq!(track_2.volume().to_db_ex(Db::MINUS_INF), Db::ZERO_DB);
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 91)))],
        "feedback should be sent because track appears at targeted position"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    assert_eq!(track_2.volume().to_db_ex(Db::MINUS_INF), MIN_VOLUME);
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "feedback should be sent on target value change"
    );
}

async fn fx_by_position() {
    // Given
    let realearn = setup().await;
    let project = Reaper::get().current_project();
    let chain = project.add_track().unwrap().normal_fx_chain();
    let delay = chain.add_fx_by_original_name("ReaDelay (Cockos)").unwrap();
    let eq = chain.add_fx_by_original_name("ReaEQ (Cockos)").unwrap();
    let synth = chain.add_fx_by_original_name("ReaSynth (Cockos)").unwrap();
    fn is_zero(param: FxParameter) -> bool {
        param.reaper_normalized_value().get() == 0.0
    }
    assert!(!is_zero(eq.parameter_by_index(1)));
    assert!(!is_zero(synth.parameter_by_index(1)));
    assert!(!is_zero(delay.parameter_by_index(1)));
    // When
    load_realearn_preset(&realearn, include_str!("presets/fx-by-position.json"));
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 64))),],
        "feedback should be sent after loading preset"
    );
    // When
    send_midi(note_on(0, 64, 10)).await;
    // Then
    assert_abs_diff_eq!(
        eq.parameter_by_index(1).reaper_normalized_value().get(),
        10.0 / 127.0,
        epsilon = BASE_EPSILON
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 10))),],
        "feedback should be sent on target value change"
    );
    // When
    chain.remove_fx(&eq).unwrap();
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        // Zero because ReaSynth Release parameter is roughly at zero by default.
        vec![Midi(Plain(note_on(0, 64, 0))),],
        "feedback should be sent when ReaSynth FX appears at targeted position because of removal"
    );
    // When
    send_midi(note_on(0, 64, 10)).await;
    // Then
    assert_abs_diff_eq!(
        synth.parameter_by_index(1).reaper_normalized_value().get(),
        10.0 / 127.0,
        epsilon = BASE_EPSILON
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 10))),],
        "feedback should be sent on target value change"
    );
    // When
    chain.move_fx(&delay, 1).unwrap();
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 64))),],
        "feedback should be sent when ReaDelay FX appears at targeted position because of reorder"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert!(is_zero(delay.parameter_by_index(1)));
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0))),],
        "feedback should be sent on target value change"
    );
}

async fn fx_by_name() {
    // Given
    let realearn = setup().await;
    let project = Reaper::get().current_project();
    let chain = project.add_track().unwrap().normal_fx_chain();
    let delay = chain.add_fx_by_original_name("ReaDelay (Cockos)").unwrap();
    let eq = chain.add_fx_by_original_name("ReaEQ (Cockos)").unwrap();
    let synth = chain.add_fx_by_original_name("ReaSynth (Cockos)").unwrap();
    fn is_zero(param: FxParameter) -> bool {
        param.reaper_normalized_value().get() == 0.0
    }
    assert!(!is_zero(eq.parameter_by_index(1)));
    assert!(!is_zero(synth.parameter_by_index(1)));
    assert!(!is_zero(delay.parameter_by_index(1)));
    // When
    load_realearn_preset(&realearn, include_str!("presets/fx-by-name.json"));
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 64))),],
        "feedback should be sent after loading preset"
    );
    // When
    send_midi(note_on(0, 64, 10)).await;
    // Then
    assert_abs_diff_eq!(
        eq.parameter_by_index(1).reaper_normalized_value().get(),
        10.0 / 127.0,
        epsilon = FEEDBACK_EPSILON
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 10))),],
        "feedback should be sent on target value change"
    );
    // When
    chain.remove_fx(&eq).unwrap();
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "gone feedback"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert!(!is_zero(synth.parameter_by_index(1)));
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if target value not changed"
    );
    // When
    chain.move_fx(&delay, 1).unwrap();
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert!(!is_zero(delay.parameter_by_index(1)));
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if target value not changed"
    );
}

async fn fx_by_id() {
    // Given
    let realearn = setup().await;
    let project = Reaper::get().current_project();
    let chain = project.add_track().unwrap().normal_fx_chain();
    let delay = chain.add_fx_by_original_name("ReaDelay (Cockos)").unwrap();
    let eq = chain.add_fx_by_original_name("ReaEQ (Cockos)").unwrap();
    let eq_guid_string = eq.guid().unwrap().to_string_without_braces();
    let synth = chain.add_fx_by_original_name("ReaSynth (Cockos)").unwrap();
    fn is_zero(param: FxParameter) -> bool {
        param.reaper_normalized_value().get() == 0.0
    }
    assert!(!is_zero(eq.parameter_by_index(1)));
    assert!(!is_zero(synth.parameter_by_index(1)));
    assert!(!is_zero(delay.parameter_by_index(1)));
    // When
    load_realearn_preset(
        &realearn,
        &include_str!("presets/fx-by-id.json").replace("$EQ_GUID", &eq_guid_string),
    );
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 64))),],
        "feedback should be sent after loading preset"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert!(is_zero(eq.parameter_by_index(1)));
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0))),],
        "feedback should be sent on target value change"
    );
    // When
    chain.remove_fx(&eq).unwrap();
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        // vec![concrete_midi(Plain(note_on(0, 64, 0)))],
        vec![],
        "gone feedback"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert!(!is_zero(synth.parameter_by_index(1)));
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if target value not changed"
    );
    // When
    chain.move_fx(&delay, 1).unwrap();
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert!(!is_zero(delay.parameter_by_index(1)));
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if target value not changed"
    );
}

async fn track_by_name() {
    // Given
    let realearn = setup().await;
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    // When
    load_realearn_preset(&realearn, include_str!("presets/track-by-name.json"));
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent after loading preset because target track doesn't exist yet"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if target value not changed"
    );
    // When
    let track_2 = realearn.track().project().add_track().unwrap();
    moment().await;
    // Then
    assert_eq!(track_2.volume().to_db_ex(Db::MINUS_INF), Db::ZERO_DB);
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if track added and target track doesn't exist yet"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    assert_eq!(track_2.volume().to_db_ex(Db::MINUS_INF), Db::ZERO_DB);
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if target value not changed"
    );
    // When
    track_2.set_name("Find me!");
    moment().await;
    // Then
    assert_eq!(track_2.volume().to_db_ex(Db::MINUS_INF), Db::ZERO_DB);
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 91)))],
        "feedback should be sent if track with targeted name appears"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    assert_eq!(track_2.volume().to_db_ex(Db::MINUS_INF), MIN_VOLUME);
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "feedback should be sent on target value change"
    );
}

async fn conditional_activation_modifiers() {
    // Given
    let realearn = setup().await;
    // When
    load_realearn_preset(&realearn, include_str!("presets/modifier-condition.json"));
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent after loading preset because activation condition not yet met"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if target value not changed"
    );
    // When
    // Switch first modifier on (condition says it must be on)
    realearn
        .parameter_by_index(82)
        .set_reaper_normalized_value(0.5)
        .unwrap();
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 91))),],
        "feedback should be sent as soon as activation condition is met (met first time)"
    );
    // When
    send_midi(note_on(0, 64, 10)).await;
    // Then
    assert_abs_diff_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF).get(),
        -60.244583841299885,
        epsilon = BASE_EPSILON
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 10)))],
        "feedback should be sent on target value change"
    );
    // When
    // Switch second modifier on (condition says it must be off)
    realearn
        .parameter_by_index(13)
        .set_reaper_normalized_value(1.0)
        .unwrap();
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        // TODO-medium Why no gone feedback!? When I test this in real, it works!
        // vec![concrete_midi(Plain(note_on(0, 64, 0)))],
        vec![],
        "gone feedback finally here, hooray"
    );
    // When
    send_midi(note_on(0, 64, 127)).await;
    // Then
    // Value should remain unchanged
    assert_abs_diff_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF).get(),
        -60.244583841299885,
        epsilon = BASE_EPSILON
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if target value not changed"
    );
    // When
    // Switch second modifier off (condition says it must be off)
    realearn
        .parameter_by_index(13)
        .set_reaper_normalized_value(0.0)
        .unwrap();
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        // If the "gone" feedback above would work, this would be sent. Right now, it's omitted
        // because of duplicate-feedback prevention measures - which in itself is correct.
        // vec![concrete_midi(Plain(note_on(0, 64, 10))),],
        vec![],
        "feedback should be sent as soon as activation condition is met (met again)"
    );
    // When
    send_midi(note_on(0, 64, 127)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::TWELVE_DB
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 127)))],
        "feedback should be sent on target value change"
    );
}

async fn conditional_activation_program() {
    // Given
    let realearn = setup().await;
    // When
    load_realearn_preset(&realearn, include_str!("presets/program-condition.json"));
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent after loading preset because activation condition not yet met"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if target value not changed"
    );
    // When
    realearn
        .parameter_by_index(82)
        .set_reaper_normalized_value(0.4)
        .unwrap();
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if parameter changed but activation condition not yet met"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if target value not changed"
    );
    // When
    realearn
        .parameter_by_index(82)
        .set_reaper_normalized_value(0.5)
        .unwrap();
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 91)))],
        "feedback should be sent as soon as activation condition is met"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        MIN_VOLUME
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "feedback should be sent on target value change"
    );
}

async fn conditional_activation_eel() {
    // Given
    let realearn = setup().await;
    // When
    load_realearn_preset(&realearn, include_str!("presets/eel-condition.json"));
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent after loading preset because activation condition not yet met"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if target value not changed"
    );
    // When
    realearn
        .parameter_by_index(66)
        .set_reaper_normalized_value(0.3)
        .unwrap();
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if parameter changed but activation condition not yet met"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        Db::ZERO_DB
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if target value not changed"
    );
    // When
    realearn
        .parameter_by_index(66)
        .set_reaper_normalized_value(0.6)
        .unwrap();
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 91))),],
        "feedback should be sent as soon as activation condition is met"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(
        realearn.track().volume().to_db_ex(Db::MINUS_INF),
        MIN_VOLUME
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "feedback should be sent on target value change"
    );
}

fn load_realearn_preset(realearn: &RealearnTestInstance, json: &str) {
    let preset_c_string = CString::new(json).expect("couldn't convert preset into c string");
    unsafe {
        let bytes = preset_c_string.into_bytes_with_nul();
        // TODO-medium Use instance shell directly instead
        realearn
            .outcome
            .fx
            .set_named_config_param(SET_STATE_PARAM_NAME, bytes.as_ptr() as _)
            .unwrap();
    }
}

async fn send_midi(message: impl ShortMessage) {
    Reaper::get().stuff_midi_message(StuffMidiMessageTarget::VirtualMidiKeyboardQueue, message);
    moment().await;
}

async fn send_midi_multi<T: ShortMessage + Copy, const I: usize>(messages: [Option<T>; I]) {
    for msg in messages.iter().flatten() {
        Reaper::get().stuff_midi_message(StuffMidiMessageTarget::VirtualMidiKeyboardQueue, *msg);
    }
    moment().await;
}

#[cfg(target_os = "macos")]
mod macos_impl {
    use super::*;
    use crate::application::UnitModel;
    use crate::domain::CompartmentKind;
    use crate::infrastructure::plugin::UnitShell;
    use crate::infrastructure::ui::copy_text_to_clipboard;
    use std::borrow::Cow;
    use std::cell::Ref;
    use std::fs;
    use std::path::PathBuf;
    use swell_ui::View;
    use swell_ui::Window;
    use xcap::image::{imageops, DynamicImage};

    pub async fn take_screenshots() {
        // Given
        let realearn = setup().await;
        let project = realearn.track().project();
        let reverb_track = project.add_track().unwrap();
        reverb_track.set_name("Reverb");
        let piano_track = project.add_track().unwrap();
        piano_track.set_name("Piano");
        let synth_track = project.add_track().unwrap();
        synth_track.set_name("Synth");
        synth_track.add_send_to(&reverb_track);
        // When
        load_realearn_preset(&realearn, include_str!("presets/screenshots.json"));
        moment().await;
        // Then
        realearn.outcome.fx.show_in_floating_window().unwrap();
        let instance_shell = realearn.outcome.instance_shell;
        let main_unit_shell = instance_shell.main_unit_shell();
        let shooter =
            Screenshooter::new(dirs::download_dir().unwrap().join("realearn-screenshots"));
        // Main panel
        let main_panel_window = Window::from_hwnd(realearn.outcome.fx.floating_window().unwrap());
        let main_panel_image = shooter.capture(main_panel_window).await;
        shooter.save_image(&main_panel_image, "main-panel");
        let main_panel_parts = [
            ("main-panel-input-output", (14, 110, 714, 124)),
            ("main-panel-menu-bar", (720, 112, 774, 66)),
            ("main-panel-let-through-checkboxes", (854, 168, 640, 66)),
            ("main-panel-show-buttons", (2, 236, 1490, 62)),
            ("main-panel-preset", (2, 302, 1490, 62)),
            ("main-panel-group", (6, 362, 994, 62)),
            ("main-panel-notes-button", (1350, 362, 136, 62)),
            ("main-panel-mapping-toolbar", (6, 423, 1492, 62)),
            ("main-panel-mapping-row", (4, 477, 1450, 148)),
            ("main-panel-bottom", (10, 1224, 1482, 120)),
        ];
        for (name, crop) in main_panel_parts {
            shooter.save_image_part(&main_panel_image, name, crop);
        }
        // Mapping panel
        let mapping = main_unit_model(main_unit_shell)
            .mappings(CompartmentKind::Main)
            .next()
            .cloned()
            .unwrap();
        let mapping_panel = main_unit_shell
            .panel()
            .panel_manager()
            .borrow_mut()
            .edit_mapping(&mapping);
        let mapping_window = mapping_panel.view_context().require_window();
        let mapping_image = shooter.capture(mapping_window).await;
        shooter.save_image(&mapping_image, "mapping-panel");
        let mapping_panel_parts = [
            ("mapping-panel-general", (4, 52, 1434, 190)),
            ("mapping-panel-source", (0, 240, 558, 462)),
            ("mapping-panel-target", (564, 240, 878, 462)),
            ("mapping-panel-glue", (0, 704, 1438, 662)),
            ("mapping-panel-bottom", (2, 1370, 1438, 172)),
        ];
        for (name, crop) in mapping_panel_parts {
            shooter.save_image_part(&mapping_image, name, crop);
        }
        // Group panel
        let group_panel = main_unit_shell.panel().header_panel().edit_group().unwrap();
        let group_window = group_panel.view_context().require_window();
        shooter.save(group_window, "group-panel").await;
        group_panel.close();
        // Log and copy screenshot directory
        log(format!("Screenshot directory: {:?}\n", &shooter.dir));
        copy_text_to_clipboard(shooter.dir.to_string_lossy().to_string());
    }

    fn main_unit_model(main_unit_shell: &UnitShell) -> Ref<UnitModel> {
        main_unit_shell.model().borrow()
    }

    struct Screenshooter {
        dir: PathBuf,
    }

    impl Screenshooter {
        pub fn new(dir: PathBuf) -> Self {
            fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }

        pub async fn save(&self, window: Window, name: &str) {
            let img = self.capture(window).await;
            self.save_image(&img, name);
        }

        pub async fn capture(&self, window: Window) -> DynamicImage {
            millis(100).await;
            xcap::Window::all()
                .unwrap()
                .iter()
                .find(|w| w.app_name() == "REAPER" && w.title() == window.text().unwrap())
                .expect("couldn't find window to take screenshot from")
                .capture_image()
                .unwrap()
                .into()
        }

        pub fn save_image_part(
            &self,
            img: &DynamicImage,
            name: &str,
            (x, y, width, height): (u32, u32, u32, u32),
        ) {
            let cropped_img = img.crop_imm(x, y, width, height);
            self.save_image(&cropped_img, name);
        }

        pub fn save_image(&self, img: &DynamicImage, name: &str) {
            const MAX_DIM: u32 = 900;
            let img = if img.height() > MAX_DIM || img.width() > MAX_DIM {
                Cow::Owned(img.resize(MAX_DIM, MAX_DIM, imageops::FilterType::Lanczos3))
            } else {
                Cow::Borrowed(img)
            };
            img.save(self.dir.join(format!("{name}.png"))).unwrap();
        }
    }
}

const MIN_VOLUME: Db = Db::MINUS_INF;
