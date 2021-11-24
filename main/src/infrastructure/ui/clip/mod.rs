use crate::application::{Session, SharedSession, WeakSession};
use crate::domain::{ClipChangedEvent, ClipPlayState, ClipSlotUpdatedEvent};
use crate::infrastructure::ui::clip::egui_clip_view::EguiClipView;
use crate::infrastructure::ui::clip::sixtyfps_clip_view::SixtyFpsClipView;
use std::rc::Rc;

mod egui_clip_view;
mod sixtyfps_clip_view;

pub type ClipView = FlexibleClipView<EguiClipView>;

#[derive(Debug)]
pub struct FlexibleClipView<T: ViewClips> {
    view: T,
}

impl<T: ViewClips> FlexibleClipView<T> {
    pub fn new(weak_session: WeakSession) -> Self {
        Self {
            view: T::new(weak_session),
        }
    }

    pub fn show(&self) {
        self.view.show();
    }

    pub fn clip_slots_updated(&self, session: &Session, events: Vec<ClipSlotUpdatedEvent>) {
        self.view.clip_slots_updated(session, events);
    }
}

pub trait ViewClips {
    fn new(weak_session: WeakSession) -> Self;

    fn show(&self);

    fn clip_slots_updated(&self, session: &Session, events: Vec<ClipSlotUpdatedEvent>);
}
