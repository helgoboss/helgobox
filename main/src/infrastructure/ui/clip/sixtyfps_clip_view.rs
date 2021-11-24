use crate::application::{Session, WeakSession};
use crate::domain::{ClipChangedEvent, ClipPlayState, ClipSlotUpdatedEvent};
use crate::infrastructure::ui::clip::ViewClips;
use derivative::Derivative;
use sixtyfps::{Model, Timer, TimerMode};
include!(env!("SIXTYFPS_INCLUDE_GENERATED"));

#[derive(Derivative)]
#[derivative(Debug)]
pub struct SixtyFpsClipView {
    #[derivative(Debug = "ignore")]
    window: MainWindow,
}

impl ViewClips for SixtyFpsClipView {
    fn new(weak_session: WeakSession) -> Self {
        let window = MainWindow::new();

        let mut tiles: Vec<TileData> = window.get_memory_tiles().iter().collect();
        let tiles_model = std::rc::Rc::new(sixtyfps::VecModel::from(tiles));
        window.set_memory_tiles(sixtyfps::ModelHandle::new(tiles_model.clone()));

        let weak_window = window.as_weak();
        window.on_check_if_pair_solved(move || {
            let w = weak_window.upgrade().unwrap();
            let s = weak_session.upgrade().unwrap();
            let s = s.borrow();
            let instance_state = s.instance_state();
            let instance_state = instance_state.borrow();
            for (i, mut t) in tiles_model.iter().enumerate() {
                if let Ok(slot) = instance_state.get_slot(i) {
                    t.filled = slot.is_filled();
                    t.progress = if let Ok(p) = slot.position() {
                        p.get() as _
                    } else {
                        0.0
                    };
                } else {
                    t.filled = false;
                }
                tiles_model.set_row_data(i, t);
            }
        });
        Self { window }
    }

    fn show(&self) {
        self.window.run();
    }

    fn clip_slots_updated(&self, session: &Session, events: Vec<ClipSlotUpdatedEvent>) {
        let tile_datas = self.window.get_memory_tiles();
        for e in events {
            match e.clip_changed_event {
                ClipChangedEvent::PlayState(ClipPlayState::Stopped) => {
                    let tile_data = TileData {
                        filled: true,
                        progress: 0.0,
                    };
                    tile_datas.set_row_data(e.slot_index, tile_data);
                }
                ClipChangedEvent::ClipPosition(p) => {
                    let tile_data = TileData {
                        filled: true,
                        progress: p.get() as _,
                    };
                    tile_datas.set_row_data(e.slot_index, tile_data);
                }
                _ => {}
            }
        }
    }
}
