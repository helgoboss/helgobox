use crate::domain::TrackExclusivity;

pub trait HierarchyEntryProvider {
    type Entry;

    fn find_entry_by_index(&self, index: u32) -> Option<Self::Entry>;
    fn entry_count(&self) -> u32;
}

pub trait HierarchyEntry: PartialEq {
    fn folder_depth_change(&self) -> i32;
}

pub fn handle_exclusivity<E: HierarchyEntry>(
    provider: impl HierarchyEntryProvider<Entry = E>,
    exclusivity: TrackExclusivity,
    current_index: u32,
    current_entry: &E,
    mut apply: impl FnMut(u32, &E),
) {
    use TrackExclusivity::*;
    match exclusivity {
        NonExclusive => {}
        ExclusiveAll => {
            for i in 0..provider.entry_count() {
                let e = provider.find_entry_by_index(i).unwrap();
                if &e == current_entry {
                    continue;
                }
                apply(i, &e);
            }
        }
        ExclusiveFolder => {
            // At first look at tracks above
            {
                let mut delta = 0;
                for i in (0..current_index).rev() {
                    let e = provider.find_entry_by_index(i).unwrap();
                    delta -= e.folder_depth_change();
                    if delta < 0 {
                        // Reached parent folder
                        break;
                    }
                    if delta == 0 {
                        // Same level
                        apply(i, &e);
                    }
                }
            }
            // Then look at current track and tracks below.
            let current_track_depth_change = current_entry.folder_depth_change();
            if current_track_depth_change >= 0 {
                // Current track is not the last one in the folder, so look further.
                // delta will starts with 1 if the current track is a folder.
                let mut delta = current_track_depth_change;
                for i in (current_index + 1)..provider.entry_count() {
                    let e = match provider.find_entry_by_index(i) {
                        None => break,
                        Some(t) => t,
                    };
                    if delta <= 0 {
                        // Same level, maybe last track in folder
                        apply(i, &e);
                    }
                    if delta < 0 {
                        // Last track in folder
                        break;
                    }
                    delta += e.folder_depth_change();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_scenario_1() {}

    struct TestProvider(Vec<TestEntry>);

    #[derive(Copy, Clone, PartialEq)]
    struct TestEntry(i32);

    impl HierarchyEntryProvider for TestProvider {
        type Entry = TestEntry;

        fn find_entry_by_index(&self, index: u32) -> Option<Self::Entry> {
            self.0.get(index as usize).copied()
        }

        fn entry_count(&self) -> u32 {
            self.0.len() as _
        }
    }

    impl HierarchyEntry for TestEntry {
        fn folder_depth_change(&self) -> i32 {
            self.0
        }
    }
}
