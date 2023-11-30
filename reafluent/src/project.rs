use crate::{Reaper, Track};
use reaper_medium::{ProjectContext, ReaProject};
use std::iter::FusedIterator;
use std::marker::PhantomData;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct ProjectDesc {
    raw: ReaProject,
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Project<'a> {
    raw: ReaProject,
    _p: PhantomData<&'a ()>,
}

impl ProjectDesc {
    pub(crate) fn new(raw: ReaProject) -> Self {
        Self { raw }
    }

    pub fn resolve(&self) -> Option<Project> {
        if !Reaper::get()
            .medium_reaper()
            .validate_ptr_2(ProjectContext::CurrentProject, self.raw)
        {
            return None;
        }
        Some(Project::new(self.raw))
    }
}

impl<'a> Project<'a> {
    pub fn new(raw: ReaProject) -> Self {
        Self {
            raw,
            _p: PhantomData,
        }
    }

    pub fn desc(&self) -> ProjectDesc {
        ProjectDesc::new(self.raw())
    }

    pub fn raw(&self) -> ReaProject {
        self.raw
    }

    pub fn tracks(
        &self,
    ) -> impl Iterator<Item = Track<'a>> + ExactSizeIterator + FusedIterator + DoubleEndedIterator + '_
    {
        let r = Reaper::get().medium_reaper();
        (0..self.track_count()).map(|i| {
            let media_track = r.get_track(self.context(), i).expect("must exist");
            Track::new(media_track)
        })
    }

    pub fn track_count(&self) -> u32 {
        Reaper::get().medium_reaper().count_tracks(self.context())
    }

    pub fn context(&self) -> ProjectContext {
        ProjectContext::Proj(self.raw)
    }
}
