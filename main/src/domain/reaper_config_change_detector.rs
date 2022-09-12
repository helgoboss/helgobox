use reaper_high::Reaper;

#[derive(Debug, Default)]
pub struct ReaperConfigChangeDetector {
    project_options: ProjectOptions,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct ProjectOptions {
    pub run_background_projects: bool,
    pub run_stopped_background_projects: bool,
}

#[derive(Debug)]
pub enum ReaperConfigChange {
    ProjectOptions(ProjectOptions),
}

impl ReaperConfigChangeDetector {
    pub fn poll_for_changes(&mut self) -> Vec<ReaperConfigChange> {
        let mut changes = vec![];
        let project_options = get_project_options();
        if project_options != self.project_options {
            self.project_options = project_options;
            changes.push(ReaperConfigChange::ProjectOptions(project_options));
        }
        changes
    }
}

pub fn get_project_options() -> ProjectOptions {
    if let Some(res) = Reaper::get().medium_reaper().get_config_var("multiprojopt") {
        assert!(res.size > 0, "multiprojopt value should have size > 0");
        let bit_mask = unsafe { res.value.cast::<u8>().as_ref() };
        ProjectOptions {
            // Bit 0 = disable_background_projects
            run_background_projects: (*bit_mask & (1 << 0)) == 0,
            // Bit 1 = enable_stopped_background_projects
            run_stopped_background_projects: (*bit_mask & (1 << 1)) != 0,
        }
    } else {
        Default::default()
    }
}
