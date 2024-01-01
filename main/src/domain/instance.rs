use crate::domain::{Backbone, ProcessorContext, UnitId};
use base::{NamedChannelSender, SenderToNormalThread};
use reaper_high::ChangeEvent;
use std::cell::RefCell;
use std::fmt;
use std::num::ParseIntError;
use std::rc::{Rc, Weak};
use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};

pub type SharedInstance = Rc<RefCell<Instance>>;
pub type WeakInstance = Weak<RefCell<Instance>>;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct InstanceId(u32);

#[derive(Debug)]
pub struct Instance {
    id: InstanceId,
    processor_context: ProcessorContext,
    main_unit_id: UnitId,
    handler: Box<dyn InstanceHandler>,
    #[cfg(feature = "playtime")]
    clip_matrix_ref: Option<ClipMatrixRef>,
    #[cfg(feature = "playtime")]
    pub clip_matrix_event_sender: SenderToNormalThread<QualifiedClipMatrixEvent>,
    #[cfg(feature = "playtime")]
    pub audio_hook_task_sender: base::SenderToRealTimeThread<crate::domain::NormalAudioHookTask>,
    #[cfg(feature = "playtime")]
    pub real_time_instance_task_sender:
        base::SenderToRealTimeThread<crate::domain::RealTimeInstanceTask>,
}

pub trait InstanceHandler: fmt::Debug {
    #[cfg(feature = "playtime")]
    fn clip_matrix_changed(
        &self,
        instance_id: InstanceId,
        matrix: &playtime_clip_engine::base::Matrix,
        events: &[playtime_clip_engine::base::ClipMatrixEvent],
        is_poll: bool,
    );
    #[cfg(feature = "playtime")]
    fn process_control_surface_change_event_for_clip_engine(
        &self,
        instance_id: InstanceId,
        matrix: &playtime_clip_engine::base::Matrix,
        events: &[reaper_high::ChangeEvent],
    );
}

impl Drop for Instance {
    fn drop(&mut self) {
        Backbone::get().unregister_instance(&self.id);
    }
}

#[cfg(feature = "playtime")]
#[derive(Debug)]
pub enum ClipMatrixRef {
    Own(Box<playtime_clip_engine::base::Matrix>),
    Foreign(InstanceId),
}

#[cfg(feature = "playtime")]
#[derive(Debug)]
pub struct QualifiedClipMatrixEvent {
    pub instance_id: InstanceId,
    pub event: playtime_clip_engine::base::ClipMatrixEvent,
}

impl fmt::Display for InstanceId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for InstanceId {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse()?))
    }
}

impl InstanceId {
    pub fn next() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl From<u32> for InstanceId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<InstanceId> for u32 {
    fn from(value: InstanceId) -> Self {
        value.0
    }
}

impl Instance {
    pub fn new(
        main_unit_id: UnitId,
        processor_context: ProcessorContext,
        handler: Box<dyn InstanceHandler>,
        #[cfg(feature = "playtime")] clip_matrix_event_sender: SenderToNormalThread<
            QualifiedClipMatrixEvent,
        >,
        #[cfg(feature = "playtime")] audio_hook_task_sender: base::SenderToRealTimeThread<
            crate::domain::NormalAudioHookTask,
        >,
        #[cfg(feature = "playtime")] real_time_instance_task_sender: base::SenderToRealTimeThread<
            crate::domain::RealTimeInstanceTask,
        >,
    ) -> Self {
        Self {
            id: InstanceId::next(),
            main_unit_id,
            handler,
            processor_context,
            #[cfg(feature = "playtime")]
            clip_matrix_ref: None,
            #[cfg(feature = "playtime")]
            clip_matrix_event_sender,
            #[cfg(feature = "playtime")]
            audio_hook_task_sender,
            #[cfg(feature = "playtime")]
            real_time_instance_task_sender,
        }
    }

    pub fn id(&self) -> InstanceId {
        self.id
    }

    pub fn main_unit_id(&self) -> UnitId {
        self.main_unit_id
    }

    /// Polls the clip matrix of this ReaLearn instance, if existing and only if it's an owned one
    /// (not borrowed from another instance).
    #[cfg(feature = "playtime")]
    pub fn poll_owned_clip_matrix(&mut self) -> Vec<playtime_clip_engine::base::ClipMatrixEvent> {
        let Some(matrix) = self.owned_clip_matrix_mut() else {
            return vec![];
        };
        let events = matrix.poll(self.processor_context.project());
        self.handler
            .clip_matrix_changed(self.id, matrix, &events, true);
        events
    }

    #[cfg(feature = "playtime")]
    pub fn process_non_polled_clip_matrix_event(
        &self,
        event: &crate::domain::QualifiedClipMatrixEvent,
    ) {
        if event.instance_id != self.id {
            return;
        }
        let Some(matrix) = self.owned_clip_matrix() else {
            return;
        };
        self.handler.clip_matrix_changed(
            self.id,
            matrix,
            std::slice::from_ref(&event.event),
            false,
        );
    }

    pub fn process_control_surface_change_events(&mut self, events: &[ChangeEvent]) {
        if events.is_empty() {
            return;
        }
        #[cfg(feature = "playtime")]
        {
            if let Some(matrix) = self.owned_clip_matrix_mut() {
                // Let matrix react to track changes etc.
                matrix.process_reaper_change_events(events);
                // Process for GUI
                self.handler
                    .process_control_surface_change_event_for_clip_engine(self.id, matrix, events);
            }
        }
    }

    #[cfg(feature = "playtime")]
    pub fn clip_matrix_relevance(&self, instance_id: InstanceId) -> Option<ClipMatrixRelevance> {
        match self.clip_matrix_ref.as_ref()? {
            ClipMatrixRef::Own(m) if instance_id == self.id => Some(ClipMatrixRelevance::Owns(m)),
            ClipMatrixRef::Foreign(id) if instance_id == *id => Some(ClipMatrixRelevance::Borrows),
            _ => None,
        }
    }

    pub fn notify_mappings_in_unit_changed(&self, unit_id: UnitId) {
        #[cfg(feature = "playtime")]
        if unit_id == self.main_unit_id {
            if let Some(matrix) = self.owned_clip_matrix() {
                matrix.notify_simple_mappings_changed();
            }
        }
    }

    pub fn notify_learning_target_in_unit_changed(&self, unit_id: UnitId) {
        #[cfg(feature = "playtime")]
        if unit_id == self.main_unit_id {
            if let Some(matrix) = self.owned_clip_matrix() {
                matrix.notify_learning_target_changed();
            }
        }
    }

    #[cfg(feature = "playtime")]
    pub fn owned_clip_matrix(&self) -> Option<&playtime_clip_engine::base::Matrix> {
        use crate::domain::ClipMatrixRef::*;
        match self.clip_matrix_ref.as_ref()? {
            Own(m) => Some(m),
            Foreign(_) => None,
        }
    }

    #[cfg(feature = "playtime")]
    pub fn owned_clip_matrix_mut(&mut self) -> Option<&mut playtime_clip_engine::base::Matrix> {
        use crate::domain::ClipMatrixRef::*;
        match self.clip_matrix_ref.as_mut()? {
            Own(m) => Some(m),
            Foreign(_) => None,
        }
    }

    #[cfg(feature = "playtime")]
    pub fn clip_matrix_ref(&self) -> Option<&ClipMatrixRef> {
        self.clip_matrix_ref.as_ref()
    }

    #[cfg(feature = "playtime")]
    pub fn clip_matrix_ref_mut(&mut self) -> Option<&mut ClipMatrixRef> {
        self.clip_matrix_ref.as_mut()
    }

    /// Returns `true` if it installed a clip matrix.
    #[cfg(feature = "playtime")]
    pub(super) fn create_and_install_owned_clip_matrix_if_necessary(
        &mut self,
        create_handler: impl FnOnce(&Instance) -> Box<dyn playtime_clip_engine::base::ClipMatrixHandler>,
    ) -> bool {
        if matches!(self.clip_matrix_ref.as_ref(), Some(ClipMatrixRef::Own(_))) {
            return false;
        }
        let matrix = playtime_clip_engine::base::Matrix::new(
            create_handler(self),
            self.processor_context.track().cloned(),
        );
        self.update_real_time_clip_matrix(Some(matrix.real_time_matrix()), true);
        self.set_clip_matrix_ref(Some(ClipMatrixRef::Own(Box::new(matrix))));
        self.clip_matrix_event_sender
            .send_complaining(QualifiedClipMatrixEvent {
                instance_id: self.id,
                event: playtime_clip_engine::base::ClipMatrixEvent::EverythingChanged,
            });
        true
    }

    #[cfg(feature = "playtime")]
    pub(super) fn set_clip_matrix_ref(&mut self, matrix_ref: Option<ClipMatrixRef>) {
        if self.clip_matrix_ref.is_some() {
            base::tracing_debug!("Shutdown existing clip matrix or remove reference to clip matrix of other instance");
            self.update_real_time_clip_matrix(None, false);
        }
        self.clip_matrix_ref = matrix_ref;
    }

    #[cfg(feature = "playtime")]
    pub(super) fn update_real_time_clip_matrix(
        &self,
        real_time_matrix: Option<playtime_clip_engine::rt::WeakRtMatrix>,
        is_owned: bool,
    ) {
        let rt_task = crate::domain::RealTimeInstanceTask::SetClipMatrix {
            is_owned,
            matrix: real_time_matrix,
        };
        self.real_time_instance_task_sender
            .send_complaining(rt_task);
    }
}

#[cfg(feature = "playtime")]
pub enum ClipMatrixRelevance<'a> {
    /// This instance owns the clip matrix with the given ID.
    Owns(&'a playtime_clip_engine::base::Matrix),
    /// This instance borrows the clip matrix with the given ID.
    Borrows,
}
