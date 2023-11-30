use crate::{ProjectDesc, TrackDesc};
use fragile::Fragile;
use reaper_medium::{ProjectContext, ProjectRef, ReaperSession, TrackDefaultsBehavior};
use std::sync::OnceLock;

// TODO-high Take care of executing Drop. Important if deployed as part of VST plug-in that
//  gets unloaded on Windows. Maybe Fragile helps here? Debug Drop invocations on Windows!
static INSTANCE: OnceLock<Reaper> = OnceLock::new();

#[derive(Debug)]
pub struct Reaper {
    medium_session: Fragile<ReaperSession>,
}

impl Reaper {
    pub fn install_globally(medium_session: ReaperSession) -> Result<(), Self> {
        let reaper = Self {
            medium_session: Fragile::new(medium_session),
        };
        INSTANCE.set(reaper)
    }

    pub fn get() -> &'static Self {
        INSTANCE
            .get()
            .expect("You must first call `Reaper::install_globally` in order to use this function.")
    }

    pub fn medium_session(&self) -> &ReaperSession {
        self.medium_session.get()
    }

    pub fn medium_reaper(&self) -> &reaper_medium::Reaper {
        self.medium_session.get().reaper()
    }

    pub fn current_project(&self) -> ProjectDesc {
        let project = Reaper::get()
            .medium_reaper()
            .enum_projects(ProjectRef::Current, 0)
            .expect("must exist")
            .project;
        ProjectDesc::new(project)
    }

    pub fn insert_track_at(&self, index: u32, behavior: TrackDefaultsBehavior) -> TrackDesc {
        let r = self.medium_reaper();
        r.insert_track_at_index(index, behavior);
        let media_track = r
            .get_track(ProjectContext::CurrentProject, index)
            .expect("impossible");
        let guid = unsafe { r.get_set_media_track_info_get_guid(media_track) };
        TrackDesc::new(self.current_project(), guid)
    }

    // pub fn with_insert_track_at<R>(
    //     &self,
    //     index: u32,
    //     behavior: TrackDefaultsBehavior,
    //     f: impl FnOnce(&Track) -> R,
    // ) -> R {
    //     let r = self.medium_reaper();
    //     r.insert_track_at_index(index, behavior);
    //     let media_track = r
    //         .get_track(ProjectContext::CurrentProject, index)
    //         .expect("impossible");
    //     f(&Track(media_track))
    // }
}
