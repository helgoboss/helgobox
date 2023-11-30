use crate::{FxChainDesc, Project, ProjectDesc, Reaper};
use reaper_low::raw::GUID;
use reaper_medium::{MediaTrack, TrackFxChainType};
use std::marker::PhantomData;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct TrackDesc {
    project_desc: ProjectDesc,
    guid: GUID,
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Track<'a> {
    raw: MediaTrack,
    _p: PhantomData<&'a ()>,
}

impl TrackDesc {
    pub fn new(project_desc: ProjectDesc, guid: GUID) -> Self {
        Self { project_desc, guid }
    }

    pub fn resolve(&self) -> Option<Track> {
        let project = self.project_desc.resolve()?;
        let track = project.tracks().find(|t| t.guid() == self.guid);
        track
    }

    pub fn normal_fx_chain(&self) -> FxChainDesc {
        FxChainDesc::new(*self, TrackFxChainType::NormalFxChain)
    }
}

impl<'a> Track<'a> {
    pub(crate) fn new(raw: MediaTrack) -> Self {
        Self {
            raw,
            _p: PhantomData,
        }
    }

    pub fn desc(&self) -> TrackDesc {
        TrackDesc::new(self.project().desc(), self.guid())
    }

    pub fn project(&self) -> Project {
        let raw_project = unsafe {
            Reaper::get()
                .medium_reaper()
                .get_set_media_track_info_get_project(self.raw)
                .expect("need REAPER >= 5.95")
        };
        Project::new(raw_project)
    }

    pub fn raw(&self) -> MediaTrack {
        self.raw
    }

    pub fn guid(&self) -> GUID {
        unsafe {
            Reaper::get()
                .medium_reaper()
                .get_set_media_track_info_get_guid(self.raw)
        }
    }
}
