use reaper_high::{ActionKind, Fx, Reaper};
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
        let realearn = self.step("Setup", setup).await;
        self.step("Do something else", || load_simple_preset(realearn))
            .await;
    }

    async fn step<T>(&mut self, label: &str, f: impl FnOnce() -> T) -> T {
        futures_timer::Delay::new(Duration::from_millis(1)).await;
        Reaper::get().show_console_msg(format!("\n\n{}. {}\n", self.current_step + 1, label));
        self.current_step += 1;
        f()
    }
}

fn setup() -> Fx {
    let reaper = Reaper::get();
    let project = reaper.create_empty_project_in_new_tab();
    let track = project.add_track();
    let realearn = track
        .normal_fx_chain()
        .add_fx_by_original_name("ReaLearn (Helgoboss)")
        .expect("couldn't find ReaLearn plug-in");
    assert_eq!(realearn.parameters().count(), 80);
    realearn
}

fn load_simple_preset(_realearn: Fx) {}
