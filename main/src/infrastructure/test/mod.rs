use crate::domain::PLUGIN_PARAMETER_COUNT;
use crate::infrastructure::plugin::SET_STATE_PARAM_NAME;
use reaper_high::{ActionKind, Fx, Reaper};
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
        let realearn = self.step("Setup", setup()).await;
        self.step("Do something else", load_simple_preset(&realearn))
            .await;
    }

    async fn step<T>(&mut self, label: &str, f: impl Future<Output = T>) -> T {
        futures_timer::Delay::new(Duration::from_millis(1)).await;
        Reaper::get().show_console_msg(format!("{}. {}\n", self.current_step + 1, label));
        self.current_step += 1;
        f.await
    }
}

async fn setup() -> Fx {
    let reaper = Reaper::get();
    let project = reaper.create_empty_project_in_new_tab();
    let track = project.add_track();
    let realearn = track
        .normal_fx_chain()
        .add_fx_by_original_name("ReaLearn (Helgoboss)")
        .expect("couldn't find ReaLearn plug-in");
    assert_eq!(realearn.parameter_count(), PLUGIN_PARAMETER_COUNT + 2);
    assert_eq!(realearn.name().to_str(), "VSTi: ReaLearn (Helgoboss)");
    realearn
}

async fn load_simple_preset(realearn: &Fx) {
    load_realearn_preset(realearn, include_str!("preset-1.json"));
}

fn load_realearn_preset(realearn: &Fx, json: &str) {
    let preset_c_string = CString::new(json).expect("couldn't convert preset into c string");
    realearn
        .set_named_config_param(SET_STATE_PARAM_NAME, &preset_c_string.into_bytes_with_nul())
        .unwrap();
}
