use crate::domain::PLUGIN_PARAMETER_COUNT;
use crate::infrastructure::plugin::SET_STATE_PARAM_NAME;
use helgoboss_midi::test_util::*;
use helgoboss_midi::ShortMessage;
use reaper_high::{ActionKind, Fx, FxParameter, Reaper};
use reaper_medium::{Db, StuffMidiMessageTarget};
use std::ffi::CString;
use std::future::Future;
use tokio::time::Duration;

pub fn register_test_action() {
    Reaper::get().register_action(
        "REALEARN_INTEGRATION_TEST",
        "[developer] ReaLearn: Run integration test",
        run_test,
        ActionKind::NotToggleable,
    );
}

fn run_test() {
    Reaper::get().spawn_in_main_thread_from_main_thread(async { Test::new().test().await })
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

async fn setup() -> Fx {
    // When
    let reaper = Reaper::get();
    let project = reaper.create_empty_project_in_new_tab();
    let track = project.add_track();
    let realearn = track
        .normal_fx_chain()
        .add_fx_by_original_name("ReaLearn (Helgoboss)")
        .expect("couldn't find ReaLearn plug-in");
    // Then
    assert_eq!(realearn.parameter_count(), PLUGIN_PARAMETER_COUNT + 2);
    assert_eq!(realearn.name().to_str(), "VSTi: ReaLearn (Helgoboss)");
    moment().await;
    realearn
}

async fn basics() {
    // Given
    let realearn = setup().await;
    load_realearn_preset(&realearn, include_str!("presets/basics.json"));
    let realearn_track = realearn.track().unwrap();
    assert_eq!(realearn_track.volume().db(), Db::ZERO_DB);
    {
        // When
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::MINUS_INF);
    }
    {
        // When
        send_midi(note_on(0, 127, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::TWELVE_DB);
    }
}

async fn virtual_mapping() {
    // Given
    let realearn = setup().await;
    load_realearn_preset(&realearn, include_str!("presets/virtual.json"));
    let realearn_track = realearn.track().unwrap();
    assert_eq!(realearn_track.volume().db(), Db::ZERO_DB);
    {
        // When
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::MINUS_INF);
    }
    {
        // When
        send_midi(note_on(0, 127, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::TWELVE_DB);
    }
}

/// Tests that non-existing track ID doesn't cause errors.
async fn track_by_id() {
    // Given
    let realearn = setup().await;
    load_realearn_preset(&realearn, include_str!("presets/track-by-id.json"));
    let realearn_track = realearn.track().unwrap();
    assert_eq!(realearn_track.volume().db(), Db::ZERO_DB);
    {
        // When
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::ZERO_DB);
    }
    let track_2 = realearn_track.project().add_track();
    moment().await;
    assert_eq!(track_2.volume().db(), Db::ZERO_DB);
    {
        // When
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::ZERO_DB);
        assert_eq!(track_2.volume().db(), Db::ZERO_DB);
    }
}

async fn track_by_position() {
    // Given
    let realearn = setup().await;
    load_realearn_preset(&realearn, include_str!("presets/track-by-position.json"));
    let realearn_track = realearn.track().unwrap();
    assert_eq!(realearn_track.volume().db(), Db::ZERO_DB);
    {
        // When
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::ZERO_DB);
    }
    let track_2 = realearn_track.project().add_track();
    moment().await;
    assert_eq!(track_2.volume().db(), Db::ZERO_DB);
    {
        // When
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::ZERO_DB);
        assert_eq!(track_2.volume().db(), Db::MINUS_INF);
    }
}

async fn fx_by_position() {
    // Given
    let realearn = setup().await;
    let project = Reaper::get().current_project();
    let chain = project.add_track().normal_fx_chain();
    let delay = chain.add_fx_by_original_name("ReaDelay (Cockos)").unwrap();
    let eq = chain.add_fx_by_original_name("ReaEQ (Cockos)").unwrap();
    let synth = chain.add_fx_by_original_name("ReaSynth (Cockos)").unwrap();
    load_realearn_preset(&realearn, include_str!("presets/fx-by-position.json"));
    fn is_zero(param: FxParameter) -> bool {
        param.reaper_normalized_value().unwrap().get() == 0.0
    }
    assert!(!is_zero(eq.parameter_by_index(1)));
    assert!(!is_zero(synth.parameter_by_index(1)));
    assert!(!is_zero(delay.parameter_by_index(1)));
    {
        // When
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert!(is_zero(eq.parameter_by_index(1)));
    }
    {
        // When
        chain.remove_fx(&eq);
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert!(is_zero(synth.parameter_by_index(1)));
    }
    {
        // When
        chain.move_fx(&delay, 1);
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert!(is_zero(delay.parameter_by_index(1)));
    }
}

async fn fx_by_name() {
    // Given
    let realearn = setup().await;
    let project = Reaper::get().current_project();
    let chain = project.add_track().normal_fx_chain();
    let delay = chain.add_fx_by_original_name("ReaDelay (Cockos)").unwrap();
    let eq = chain.add_fx_by_original_name("ReaEQ (Cockos)").unwrap();
    let synth = chain.add_fx_by_original_name("ReaSynth (Cockos)").unwrap();
    load_realearn_preset(&realearn, include_str!("presets/fx-by-name.json"));
    fn is_zero(param: FxParameter) -> bool {
        param.reaper_normalized_value().unwrap().get() == 0.0
    }
    assert!(!is_zero(eq.parameter_by_index(1)));
    assert!(!is_zero(synth.parameter_by_index(1)));
    assert!(!is_zero(delay.parameter_by_index(1)));
    {
        // When
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert!(is_zero(eq.parameter_by_index(1)));
    }
    {
        // When
        chain.remove_fx(&eq);
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert!(!is_zero(synth.parameter_by_index(1)));
    }
    {
        // When
        chain.move_fx(&delay, 1);
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert!(!is_zero(delay.parameter_by_index(1)));
    }
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
    load_realearn_preset(
        &realearn,
        &include_str!("presets/fx-by-id.json").replace("$EQ_GUID", &eq_guid_string),
    );
    fn is_zero(param: FxParameter) -> bool {
        param.reaper_normalized_value().unwrap().get() == 0.0
    }
    assert!(!is_zero(eq.parameter_by_index(1)));
    assert!(!is_zero(synth.parameter_by_index(1)));
    assert!(!is_zero(delay.parameter_by_index(1)));
    {
        // When
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert!(is_zero(eq.parameter_by_index(1)));
    }
    {
        // When
        chain.remove_fx(&eq);
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert!(!is_zero(synth.parameter_by_index(1)));
    }
    {
        // When
        chain.move_fx(&delay, 1);
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert!(!is_zero(delay.parameter_by_index(1)));
    }
}

async fn track_by_name() {
    // Given
    let realearn = setup().await;
    load_realearn_preset(&realearn, include_str!("presets/track-by-name.json"));
    let realearn_track = realearn.track().unwrap();
    assert_eq!(realearn_track.volume().db(), Db::ZERO_DB);
    {
        // When
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::ZERO_DB);
    }
    let track_2 = realearn_track.project().add_track();
    moment().await;
    assert_eq!(track_2.volume().db(), Db::ZERO_DB);
    {
        // When
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::ZERO_DB);
        assert_eq!(track_2.volume().db(), Db::ZERO_DB);
    }
    track_2.set_name("Find me!");
    moment().await;
    assert_eq!(track_2.volume().db(), Db::ZERO_DB);
    {
        // When
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::ZERO_DB);
        assert_eq!(track_2.volume().db(), Db::MINUS_INF);
    }
}

async fn conditional_activation_modifiers() {
    // Given
    let realearn = setup().await;
    load_realearn_preset(&realearn, include_str!("presets/modifier-condition.json"));
    let realearn_track = realearn.track().unwrap();
    {
        // When
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::ZERO_DB);
    }
    {
        // When
        realearn
            .parameter_by_index(82)
            .set_reaper_normalized_value(0.5)
            .unwrap();
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::MINUS_INF);
    }
    {
        // When
        realearn
            .parameter_by_index(13)
            .set_reaper_normalized_value(1.0)
            .unwrap();
        send_midi(note_on(0, 127, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::MINUS_INF);
    }
    {
        // When
        realearn
            .parameter_by_index(13)
            .set_reaper_normalized_value(0.0)
            .unwrap();
        send_midi(note_on(0, 127, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::TWELVE_DB);
    }
}

async fn conditional_activation_program() {
    // Given
    let realearn = setup().await;
    load_realearn_preset(&realearn, include_str!("presets/program-condition.json"));
    let realearn_track = realearn.track().unwrap();
    {
        // When
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::ZERO_DB);
    }
    {
        // When
        realearn
            .parameter_by_index(82)
            .set_reaper_normalized_value(0.4)
            .unwrap();
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::ZERO_DB);
    }
    {
        // When
        realearn
            .parameter_by_index(82)
            .set_reaper_normalized_value(0.5)
            .unwrap();
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::MINUS_INF);
    }
}

async fn conditional_activation_eel() {
    // Given
    let realearn = setup().await;
    load_realearn_preset(&realearn, include_str!("presets/eel-condition.json"));
    let realearn_track = realearn.track().unwrap();
    {
        // When
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::ZERO_DB);
    }
    {
        // When
        realearn
            .parameter_by_index(66)
            .set_reaper_normalized_value(0.3)
            .unwrap();
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::ZERO_DB);
    }
    {
        // When
        realearn
            .parameter_by_index(66)
            .set_reaper_normalized_value(0.6)
            .unwrap();
        send_midi(note_on(0, 0, 100)).await;
        // Then
        assert_eq!(realearn_track.volume().db(), Db::MINUS_INF);
    }
}

fn load_realearn_preset(realearn: &Fx, json: &str) {
    let preset_c_string = CString::new(json).expect("couldn't convert preset into c string");
    realearn
        .set_named_config_param(SET_STATE_PARAM_NAME, &preset_c_string.into_bytes_with_nul())
        .unwrap();
}

async fn send_midi(message: impl ShortMessage) {
    Reaper::get().stuff_midi_message(StuffMidiMessageTarget::VirtualMidiKeyboardQueue, message);
    moment().await;
}
