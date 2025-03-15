use crate::domain::{AnyThreadBackboneState, Backbone, ProcessorContext, RealTimeInstance, UnitId};
#[allow(unused_imports)]
use anyhow::Context;
use base::hash_util::NonCryptoHashMap;
use base::{NamedChannelSender, SenderToNormalThread, SenderToRealTimeThread};
use helgobox_api::persistence::PotFilterKind;
use pot::{
    CurrentPreset, OptFilter, PotFavorites, PotFilterExcludes, PotIntegration, PotUnit, PresetId,
    SharedRuntimePotUnit,
};
use reaper_high::{ChangeEvent, Fx};
use std::cell::{Ref, RefCell, RefMut};
use std::fmt;
use std::num::ParseIntError;
use std::rc::{Rc, Weak};
use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::RwLock;

pub type SharedInstance = Rc<RefCell<Instance>>;
pub type WeakInstance = Weak<RefCell<Instance>>;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct InstanceId(u32);

#[derive(Debug)]
pub struct Instance {
    id: InstanceId,
    processor_context: ProcessorContext,
    feedback_event_sender: SenderToNormalThread<QualifiedInstanceEvent>,
    main_unit_id: UnitId,
    #[allow(unused)]
    handler: Box<dyn InstanceHandler>,
    /// Saves the current state for Pot preset navigation.
    ///
    /// Persistent.
    pot_unit: PotUnit,
    pub audio_hook_task_sender: base::SenderToRealTimeThread<crate::domain::NormalAudioHookTask>,
    pub real_time_instance_task_sender:
        base::SenderToRealTimeThread<crate::domain::RealTimeInstanceTask>,
    custom_data: NonCryptoHashMap<String, serde_json::Value>,
    #[cfg(feature = "playtime")]
    pub playtime: PlaytimeInstance,
}

#[cfg(feature = "playtime")]
#[derive(Debug)]
pub struct PlaytimeInstance {
    clip_matrix: Option<playtime_clip_engine::base::Matrix>,
    pub clip_matrix_event_sender: SenderToNormalThread<QualifiedClipMatrixEvent>,
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
        if Backbone::is_loaded() {
            Backbone::get().unregister_instance(&self.id);
        }
    }
}

#[derive(Debug)]
pub struct QualifiedInstanceEvent {
    pub instance_id: InstanceId,
    pub event: InstanceStateChanged,
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

const REAL_TIME_INSTANCE_TASK_QUEUE_SIZE: usize = 200;

impl Instance {
    pub fn new(
        id: InstanceId,
        main_unit_id: UnitId,
        processor_context: ProcessorContext,
        feedback_event_sender: SenderToNormalThread<QualifiedInstanceEvent>,
        handler: Box<dyn InstanceHandler>,
        #[cfg(feature = "playtime")] clip_matrix_event_sender: SenderToNormalThread<
            QualifiedClipMatrixEvent,
        >,
        audio_hook_task_sender: base::SenderToRealTimeThread<crate::domain::NormalAudioHookTask>,
    ) -> (Self, RealTimeInstance) {
        let (real_time_instance_task_sender, real_time_instance_task_receiver) =
            SenderToRealTimeThread::new_channel(
                "real-time instance tasks",
                REAL_TIME_INSTANCE_TASK_QUEUE_SIZE,
            );
        let rt_instance = RealTimeInstance::new(real_time_instance_task_receiver);
        let instance = Self {
            id,
            main_unit_id,
            feedback_event_sender,
            handler,
            processor_context,
            pot_unit: Default::default(),
            #[cfg(feature = "playtime")]
            playtime: PlaytimeInstance {
                clip_matrix: None,
                clip_matrix_event_sender,
            },
            audio_hook_task_sender,
            real_time_instance_task_sender,
            custom_data: Default::default(),
        };
        (instance, rt_instance)
    }

    pub fn id(&self) -> InstanceId {
        self.id
    }

    pub fn main_unit_id(&self) -> UnitId {
        self.main_unit_id
    }

    pub fn custom_data(&self) -> &NonCryptoHashMap<String, serde_json::Value> {
        &self.custom_data
    }

    pub fn set_custom_data(&mut self, data: NonCryptoHashMap<String, serde_json::Value>) {
        self.custom_data = data;
    }
    pub fn update_custom_data_key(&mut self, key: String, value: serde_json::Value) {
        self.custom_data.insert(key, value);
    }

    /// Returns the runtime pot unit associated with this instance.
    ///
    /// If the pot unit isn't loaded yet and loading has not been attempted yet, loads it.
    ///
    /// Returns an error if the necessary pot database is not available.
    pub fn pot_unit(&mut self) -> Result<SharedRuntimePotUnit, &'static str> {
        let integration = RealearnPotIntegration::new(
            self.id,
            self.processor_context.containing_fx().clone(),
            self.feedback_event_sender.clone(),
        );
        self.pot_unit.loaded(Box::new(integration))
    }

    /// Restores a pot unit state from persistent data.
    ///
    /// This doesn't load the pot unit yet. If the ReaLearn instance never accesses the pot unit,
    /// it simply remains unloaded and its persistent state is kept. The persistent state is also
    /// kept if loading of the pot unit fails (e.g. if the necessary pot database is not available
    /// on the user's computer).
    pub fn restore_pot_unit(&mut self, state: pot::PersistentState) {
        self.pot_unit = PotUnit::unloaded(state);
    }

    /// Returns a pot unit state suitable to be saved by the persistence logic.
    pub fn save_pot_unit(&self) -> pot::PersistentState {
        self.pot_unit.persistent_state()
    }

    pub fn process_control_surface_change_events(&mut self, events: &[ChangeEvent]) {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = events;
        }
        #[cfg(feature = "playtime")]
        {
            if events.is_empty() {
                return;
            }
            if let Some(matrix) = self.playtime.clip_matrix.as_mut() {
                // Let matrix react to track changes etc.
                matrix.process_reaper_change_events(events);
                // Process for GUI
                self.handler
                    .process_control_surface_change_event_for_clip_engine(self.id, matrix, events);
            }
        }
    }

    pub fn notify_mappings_in_unit_changed(&self, unit_id: UnitId) {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = unit_id;
        }
        #[cfg(feature = "playtime")]
        if unit_id == self.main_unit_id {
            if let Some(matrix) = self.clip_matrix() {
                matrix.notify_simple_mappings_changed();
            }
        }
    }

    pub fn notify_learning_target_in_unit_changed(&self, unit_id: UnitId) {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = unit_id;
        }
        #[cfg(feature = "playtime")]
        if unit_id == self.main_unit_id {
            if let Some(matrix) = self.clip_matrix() {
                matrix.notify_learning_target_changed();
            }
        }
    }

    pub fn has_clip_matrix(&self) -> bool {
        #[cfg(feature = "playtime")]
        {
            self.playtime.clip_matrix.is_some()
        }
        #[cfg(not(feature = "playtime"))]
        {
            false
        }
    }
}

#[cfg(feature = "playtime")]
mod playtime_impl {
    use crate::domain::instance::NO_CLIP_MATRIX_SET;
    use crate::domain::{
        err_if_reaper_version_too_low_for_playtime, Instance, QualifiedClipMatrixEvent,
    };
    use anyhow::Context;
    use base::NamedChannelSender;
    use reaper_high::OrCurrentProject;

    impl Instance {
        /// Polls the Playtime matrix of this ReaLearn instance.
        pub fn poll_owned_clip_matrix(
            &mut self,
        ) -> Vec<playtime_clip_engine::base::ClipMatrixEvent> {
            let Some(matrix) = self.playtime.clip_matrix.as_mut() else {
                return vec![];
            };
            let events = matrix.poll();
            self.handler
                .clip_matrix_changed(self.id, matrix, &events, true);
            events
        }

        pub fn process_non_polled_clip_matrix_event(
            &self,
            event: &crate::domain::QualifiedClipMatrixEvent,
        ) {
            if event.instance_id != self.id {
                return;
            }
            let Some(matrix) = self.clip_matrix() else {
                return;
            };
            self.handler.clip_matrix_changed(
                self.id,
                matrix,
                std::slice::from_ref(&event.event),
                false,
            );
        }

        pub fn get_playtime_matrix(&self) -> anyhow::Result<&playtime_clip_engine::base::Matrix> {
            self.playtime
                .clip_matrix
                .as_ref()
                .context(NO_CLIP_MATRIX_SET)
        }

        pub fn get_playtime_matrix_mut(
            &mut self,
        ) -> anyhow::Result<&mut playtime_clip_engine::base::Matrix> {
            self.playtime
                .clip_matrix
                .as_mut()
                .context(NO_CLIP_MATRIX_SET)
        }

        pub fn clip_matrix(&self) -> Option<&playtime_clip_engine::base::Matrix> {
            self.playtime.clip_matrix.as_ref()
        }

        pub fn clip_matrix_mut(&mut self) -> Option<&mut playtime_clip_engine::base::Matrix> {
            self.playtime.clip_matrix.as_mut()
        }

        /// Returns `Ok(true)` if it installed a Playtime matrix and `Ok(false)` if one was installed already.
        ///
        /// # Errors
        ///
        /// Returns an error if the Playtime matrix can't be created, e.g. when on the monitoring FX chain.
        pub(crate) fn create_and_install_clip_matrix_if_necessary(
            &mut self,
            create_handler: impl FnOnce(
                &Instance,
            )
                -> Box<dyn playtime_clip_engine::base::ClipMatrixHandler>,
        ) -> anyhow::Result<bool> {
            if self.playtime.clip_matrix.is_some() {
                return Ok(false);
            }
            err_if_reaper_version_too_low_for_playtime()?;
            let track = self.processor_context.track()
                .context("Sorry, Playtime is not intended to be used from the monitoring FX chain! If you have a really good use case for that, please write to info@helgoboss.org and we will see what we can do.")?;
            let matrix =
                playtime_clip_engine::base::Matrix::new(create_handler(self), track.clone());
            self.update_real_time_clip_matrix(Some(matrix.real_time_matrix()));
            self.set_clip_matrix(Some(matrix));
            self.playtime
                .clip_matrix_event_sender
                .send_complaining(QualifiedClipMatrixEvent {
                    instance_id: self.id,
                    event: playtime_clip_engine::base::ClipMatrixEvent::EverythingChanged,
                });
            Ok(true)
        }

        pub fn set_clip_matrix(&mut self, matrix: Option<playtime_clip_engine::base::Matrix>) {
            if self.playtime.clip_matrix.is_some() {
                tracing::debug!("Shutdown existing Playtime matrix");
                self.update_real_time_clip_matrix(None);
            }
            self.playtime.clip_matrix = matrix;
        }

        pub(super) fn update_real_time_clip_matrix(
            &self,
            real_time_matrix: Option<playtime_clip_engine::rt::WeakRtMatrix>,
        ) {
            let rt_task = crate::domain::RealTimeInstanceTask::SetClipMatrix {
                matrix: real_time_matrix,
            };
            self.real_time_instance_task_sender
                .send_complaining(rt_task);
        }
    }
}

struct RealearnPotIntegration {
    instance_id: InstanceId,
    containing_fx: Fx,
    sender: SenderToNormalThread<QualifiedInstanceEvent>,
}

impl RealearnPotIntegration {
    fn new(
        instance_id: InstanceId,
        containing_fx: Fx,
        sender: SenderToNormalThread<QualifiedInstanceEvent>,
    ) -> Self {
        Self {
            instance_id,
            containing_fx,
            sender,
        }
    }

    fn emit(&self, event: InstanceStateChanged) {
        self.sender.send_complaining(QualifiedInstanceEvent {
            instance_id: self.instance_id,
            event,
        })
    }
}

impl PotIntegration for RealearnPotIntegration {
    fn favorites(&self) -> &RwLock<PotFavorites> {
        &AnyThreadBackboneState::get().pot_favorites
    }

    fn set_current_fx_preset(&self, fx: Fx, preset: CurrentPreset) {
        Backbone::target_state()
            .borrow_mut()
            .set_current_fx_preset(fx, preset);
        self.emit(InstanceStateChanged::PotStateChanged(
            PotStateChangedEvent::PresetLoaded,
        ));
    }

    fn exclude_list(&self) -> Ref<PotFilterExcludes> {
        Backbone::get().pot_filter_exclude_list()
    }

    fn exclude_list_mut(&self) -> RefMut<PotFilterExcludes> {
        Backbone::get().pot_filter_exclude_list_mut()
    }

    fn notify_preset_changed(&self, id: Option<PresetId>) {
        self.emit(InstanceStateChanged::PotStateChanged(
            PotStateChangedEvent::PresetChanged { id },
        ));
    }

    fn notify_filter_changed(&self, kind: PotFilterKind, filter: OptFilter) {
        self.emit(InstanceStateChanged::PotStateChanged(
            PotStateChangedEvent::FilterItemChanged { kind, filter },
        ));
    }

    fn notify_indexes_rebuilt(&self) {
        self.emit(InstanceStateChanged::PotStateChanged(
            PotStateChangedEvent::IndexesRebuilt,
        ));
    }

    fn protected_fx(&self) -> &Fx {
        &self.containing_fx
    }
}

#[derive(Clone, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum InstanceStateChanged {
    PotStateChanged(PotStateChangedEvent),
}

#[derive(Clone, Debug)]
pub enum PotStateChangedEvent {
    FilterItemChanged {
        kind: PotFilterKind,
        filter: OptFilter,
    },
    PresetChanged {
        id: Option<PresetId>,
    },
    IndexesRebuilt,
    PresetLoaded,
}

#[cfg(feature = "playtime")]
const NO_CLIP_MATRIX_SET: &str = "no Playtime matrix set for this instance";

#[cfg(feature = "playtime")]
pub fn err_if_reaper_version_too_low_for_playtime() -> anyhow::Result<()> {
    const MIN_REAPER_VERSION: &str = "7";
    if reaper_high::Reaper::get().version().revision() < MIN_REAPER_VERSION {
        anyhow::bail!("Please update REAPER to version {MIN_REAPER_VERSION} to access Playtime!");
    }
    Ok(())
}
