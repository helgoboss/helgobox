use crate::usage_scope::{ReadAccess, ReadScope, ReadWriteScope, WriteAccess};
use crate::{Reaper, Track};
use reaper_medium::{ProjectContext, ReaProject};
use std::iter::FusedIterator;
use std::marker::PhantomData;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct ProjectDesc {
    raw: ReaProject,
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Project<'a, UsageScope: ReadAccess> {
    raw: ReaProject,
    _p: PhantomData<&'a (UsageScope)>,
}

impl ProjectDesc {
    pub(crate) fn new(raw: ReaProject) -> Self {
        Self { raw }
    }

    pub fn resolve(&self) -> Option<Project<ReadScope>> {
        self.resolve_internal()
    }

    pub fn resolve_mut(&mut self) -> Option<Project<ReadWriteScope>> {
        self.resolve_internal()
    }

    fn resolve_internal<S: ReadAccess>(&self) -> Option<Project<S>> {
        if !Reaper::get()
            .medium_reaper()
            .validate_ptr_2(ProjectContext::CurrentProject, self.raw)
        {
            return None;
        }
        Some(Project::new(self.raw))
    }
}

impl<'a, UsageScope: ReadAccess> Project<'a, UsageScope> {
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

    pub fn tracks<'b>(
        &'b self,
    ) -> impl Iterator<Item = Track<'b>> + ExactSizeIterator + FusedIterator + DoubleEndedIterator + 'b
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

impl<'a, UsageScope: WriteAccess> Project<'a, UsageScope> {
    pub fn delete_track(&mut self, track: Track) {
        unsafe {
            Reaper::get().medium_reaper().delete_track(track.raw());
        }
    }
}
