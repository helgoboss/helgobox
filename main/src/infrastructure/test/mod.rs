use crate::base::Global;
use crate::domain::{SourceFeedbackValue, PLUGIN_PARAMETER_COUNT};
use crate::infrastructure::plugin::{App, SET_STATE_PARAM_NAME};
use approx::assert_abs_diff_eq;
use helgoboss_learn::{MidiSourceValue, FEEDBACK_EPSILON};
use helgoboss_midi::test_util::*;
use helgoboss_midi::{DataEntryByteOrder, ParameterNumberMessage, RawShortMessage, ShortMessage};
use reaper_high::{ActionKind, Fx, FxParameter, Reaper, Track};
use reaper_medium::{Db, ReaperPanValue, StuffMidiMessageTarget};
use std::ffi::CString;
use std::future::Future;
use tokio::time::Duration;
use MidiSourceValue::{ParameterNumber, Plain};
use SourceFeedbackValue::Midi;

pub fn register_test_action() {
    Reaper::get().register_action(
        "REALEARN_INTEGRATION_TEST",
        "[developer] ReaLearn: Run integration test",
        run_test,
        ActionKind::NotToggleable,
    );
}

fn run_test() {
    Global::future_support()
        .spawn_in_main_thread_from_main_thread(async { Test::new().test().await })
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

async fn millis(amount: u64) {
    futures_timer::Delay::new(Duration::from_millis(amount)).await;
}

async fn setup() -> RealearnTestInstance {
    // When
    let reaper = Reaper::get();
    let project = reaper.create_empty_project_in_new_tab();
    let track = project.add_track();
    let fx = track
        .normal_fx_chain()
        .add_fx_by_original_name("ReaLearn (Helgoboss)")
        .expect("couldn't find ReaLearn plug-in");
    // Then
    assert_eq!(fx.parameter_count(), PLUGIN_PARAMETER_COUNT + 2);
    assert_eq!(fx.name().to_str(), "VSTi: ReaLearn (Helgoboss)");
    moment().await;
    let session = App::get()
        .find_session_by_containing_fx(&fx)
        .expect("couldn't find session associated with ReaLearn FX instance");
    let (feedback_sender, feedback_receiver) = crossbeam_channel::unbounded();
    session
        .borrow()
        .use_integration_test_feedback_sender(feedback_sender);
    RealearnTestInstance {
        fx,
        feedback_receiver,
    }
}

struct RealearnTestInstance {
    fx: Fx,
    feedback_receiver: crossbeam_channel::Receiver<SourceFeedbackValue>,
}

impl RealearnTestInstance {
    /// Returns the containing track.
    pub fn track(&self) -> &Track {
        self.fx.track().unwrap()
    }

    /// Returns the ReaLearn VST parameter at the given index.
    pub fn parameter_by_index(&self, index: u32) -> FxParameter {
        self.fx.parameter_by_index(index)
    }

    /// Returns all recorded feedback and removes it from the list.
    fn pop_feedback(&self) -> Vec<SourceFeedbackValue> {
        self.feedback_receiver.try_iter().collect()
    }
}

async fn basics() {
    // Given
    let realearn = setup().await;
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
    assert_eq!(
        realearn.track().volume().db(),
        Db::MINUS_INF,
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
    assert_eq!(realearn.track().volume().db(), Db::MINUS_INF);
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if target value not changed"
    );
    // When
    send_midi(note_on(0, 64, 127)).await;
    // Then
    assert_eq!(realearn.track().volume().db(), Db::TWELVE_DB);
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 127)))],
        "feedback should be sent on target value change"
    );
}

async fn nrpn_test() {
    // Given
    let realearn = setup().await;
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
        realearn.track().volume().db(),
        Db::MINUS_INF,
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
        realearn.track().volume().db(),
        Db::MINUS_INF,
        "increment should turn volume up a bit"
    );
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(ParameterNumber(nrpn(0, 100, 2)))],
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
        realearn.track().volume().db(),
        Db::MINUS_INF,
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
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
        ],
        "feedback should be sent after loading preset"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    send_midi(note_on(0, 63, 0)).await;
    send_midi(note_on(0, 62, 127)).await;
    // Then
    assert_eq!(
        realearn.track().volume().db(),
        Db::MIN,
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
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
        ],
        "feedback should be sent after loading preset"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    send_midi(note_on(0, 63, 0)).await;
    send_midi(note_on(0, 62, 127)).await;
    // Then
    assert_eq!(
        realearn.track().volume().db(),
        Db::MIN,
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
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
        vec![
            Midi(Plain(note_on(0, 64, 127))),
            // More than necessary
            Midi(Plain(note_on(0, 64, 127))),
        ],
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
        vec![
            Midi(Plain(note_on(0, 64, 0))),
            // More than necessary
            Midi(Plain(note_on(0, 64, 0))),
        ],
        "feedback should be sent on target value change"
    );
}

async fn send_feedback_after_control_toggle_mode_arm() {
    // Given
    let realearn = setup().await;
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
        vec![
            Midi(Plain(note_on(0, 64, 127))),
            // More than necessary
            Midi(Plain(note_on(0, 64, 127))),
            // One more because of #396 change
            Midi(Plain(note_on(0, 64, 127))),
        ],
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
        vec![
            Midi(Plain(note_on(0, 64, 127))),
            // More than necessary
            Midi(Plain(note_on(0, 64, 127))),
            // One more because of #396 change
            Midi(Plain(note_on(0, 64, 127))),
        ],
        "feedback should be sent on target value change"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert!(!realearn.track().is_armed(false));
    assert_eq!(
        realearn.pop_feedback(),
        vec![
            Midi(Plain(note_on(0, 64, 0))),
            // More than necessary
            Midi(Plain(note_on(0, 64, 0))),
            // One more because of #396 change
            Midi(Plain(note_on(0, 64, 0))),
        ],
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
            // One more because of #396 change
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
        "should send feedback even if target value not changed (after NOTE OFF)"
    );
}
async fn send_feedback_after_control_normal_mode_volume() {
    // Given
    let realearn = setup().await;
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
    assert_eq!(realearn.track().volume().db(), Db::MINUS_INF);
    assert_eq!(
        realearn.pop_feedback(),
        vec![
            Midi(Plain(note_on(0, 64, 0))),
            // One more because of #396 change
            Midi(Plain(note_on(0, 64, 0)))
        ],
        "feedback should be sent on target value change"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(realearn.track().volume().db(), Db::MINUS_INF);
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "feedback should be sent even if value still same"
    );
}

async fn basics_controller_compartment() {
    // Given
    let realearn = setup().await;
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
    assert_eq!(realearn.track().volume().db(), Db::MINUS_INF);
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "feedback should be sent on target value change"
    );
    // When
    send_midi(note_on(0, 64, 127)).await;
    // Then
    assert_eq!(realearn.track().volume().db(), Db::TWELVE_DB);
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 127)))],
        "feedback should be sent on target value change"
    );
}

async fn virtual_mapping() {
    // Given
    let realearn = setup().await;
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
    assert_eq!(realearn.track().volume().db(), Db::MINUS_INF);
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "feedback should be sent on target value change"
    );
    // When
    send_midi(note_on(0, 64, 127)).await;
    // Then
    assert_eq!(realearn.track().volume().db(), Db::TWELVE_DB);
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
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
    assert_eq!(realearn.pop_feedback(), vec![]);
    // When
    let track_2 = realearn.track().project().add_track();
    moment().await;
    // Then
    assert_eq!(track_2.volume().db(), Db::ZERO_DB);
    assert_eq!(realearn.pop_feedback(), vec![]);
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
    assert_eq!(track_2.volume().db(), Db::ZERO_DB);
    assert_eq!(realearn.pop_feedback(), vec![]);
}

async fn track_by_position() {
    // Given
    let realearn = setup().await;
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
    // When
    let track_2 = realearn.track().project().add_track();
    moment().await;
    // Then
    assert_eq!(track_2.volume().db(), Db::ZERO_DB);
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 91)))],
        "feedback should be sent because track appears at targeted position"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
    assert_eq!(track_2.volume().db(), Db::MINUS_INF);
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
    let chain = project.add_track().normal_fx_chain();
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
        vec![
            Midi(Plain(note_on(0, 64, 64))),
            Midi(Plain(note_on(0, 64, 64)))
        ],
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
        // Zero because ReaSynth Release parameter is roughly at zero by default.
        vec![Midi(Plain(note_on(0, 64, 0))),],
        "feedback should be sent when ReaSynth FX appears at targeted position because of removal"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert!(is_zero(synth.parameter_by_index(1)));
    assert_eq!(
        realearn.pop_feedback(),
        // Zero because now totally zero.
        vec![Midi(Plain(note_on(0, 64, 0))),],
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
    let chain = project.add_track().normal_fx_chain();
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
        vec![
            Midi(Plain(note_on(0, 64, 64))),
            Midi(Plain(note_on(0, 64, 64)))
        ],
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
    // TODO-medium Why no "gone" feedback?
    assert_eq!(realearn.pop_feedback(), vec![]);
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
    let chain = project.add_track().normal_fx_chain();
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
        vec![
            Midi(Plain(note_on(0, 64, 64))),
            Midi(Plain(note_on(0, 64, 64)))
        ],
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
    // TODO-medium Why no "gone" feedback?
    assert_eq!(realearn.pop_feedback(), vec![]);
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
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if target value not changed"
    );
    // When
    let track_2 = realearn.track().project().add_track();
    moment().await;
    // Then
    assert_eq!(track_2.volume().db(), Db::ZERO_DB);
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if track added and target track doesn't exist yet"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
    assert_eq!(track_2.volume().db(), Db::ZERO_DB);
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if target value not changed"
    );
    // When
    track_2.set_name("Find me!");
    moment().await;
    // Then
    assert_eq!(track_2.volume().db(), Db::ZERO_DB);
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 91)))],
        "feedback should be sent if track with targeted name appears"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
    assert_eq!(track_2.volume().db(), Db::MINUS_INF);
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
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
        vec![Midi(Plain(note_on(0, 64, 91))),],
        "feedback should be sent as soon as activation condition is met"
    );
    // When
    send_midi(note_on(0, 64, 0)).await;
    // Then
    assert_eq!(realearn.track().volume().db(), Db::MINUS_INF);
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "feedback should be sent on target value change"
    );
    // When
    realearn
        .parameter_by_index(13)
        .set_reaper_normalized_value(1.0)
        .unwrap();
    moment().await;
    // Then
    // TODO-medium Why no "gone" feedback?
    assert_eq!(realearn.pop_feedback(), vec![]);
    // When
    send_midi(note_on(0, 64, 127)).await;
    // Then
    assert_eq!(realearn.track().volume().db(), Db::MINUS_INF);
    assert_eq!(
        realearn.pop_feedback(),
        vec![],
        "no feedback should be sent if target value not changed"
    );
    // When
    realearn
        .parameter_by_index(13)
        .set_reaper_normalized_value(0.0)
        .unwrap();
    moment().await;
    // Then
    assert_eq!(
        realearn.pop_feedback(),
        // Zero because of value
        vec![Midi(Plain(note_on(0, 64, 0))),],
        "feedback should be sent as soon as activation condition is met"
    );
    // When
    send_midi(note_on(0, 64, 127)).await;
    // Then
    assert_eq!(realearn.track().volume().db(), Db::TWELVE_DB);
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
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
    assert_eq!(realearn.track().volume().db(), Db::MINUS_INF);
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
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
    assert_eq!(realearn.track().volume().db(), Db::ZERO_DB);
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
    assert_eq!(realearn.track().volume().db(), Db::MINUS_INF);
    assert_eq!(
        realearn.pop_feedback(),
        vec![Midi(Plain(note_on(0, 64, 0)))],
        "feedback should be sent on target value change"
    );
}

fn load_realearn_preset(realearn: &RealearnTestInstance, json: &str) {
    let preset_c_string = CString::new(json).expect("couldn't convert preset into c string");
    realearn
        .fx
        .set_named_config_param(SET_STATE_PARAM_NAME, &preset_c_string.into_bytes_with_nul())
        .unwrap();
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
