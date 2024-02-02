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
    provider: &impl HierarchyEntryProvider<Entry = E>,
    exclusivity: TrackExclusivity,
    current_index: Option<u32>,
    current_entry: &E,
    mut apply: impl FnMut(u32, &E),
) {
    let current_index = match current_index {
        // We consider the master track as its own folder (same as non-exclusive).
        None => return,
        Some(i) => i,
    };
    use TrackExclusivity::*;
    match exclusivity {
        NonExclusive => {}
        ExclusiveWithinProject | ExclusiveWithinProjectOnOnly => {
            for i in 0..provider.entry_count() {
                let e = provider.find_entry_by_index(i).unwrap();
                if &e == current_entry {
                    continue;
                }
                apply(i, &e);
            }
        }
        ExclusiveWithinFolder | ExclusiveWithinFolderOnOnly => {
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
                    delta += e.folder_depth_change();
                    if delta < 0 {
                        // Last track in folder
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use maplit::hashset;
    use std::collections::HashSet;
    use {TestEntry as E, TestProvider as P};

    mod exclusive_folder {
        use super::*;

        #[test]
        fn no_folders() {
            // Given
            let p = P(vec![E("-"), E("-"), E("-"), E("-")]);
            // When
            // Then
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 0,),
                hashset![1, 2, 3]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 1,),
                hashset![0, 2, 3]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 2,),
                hashset![0, 1, 3]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 3,),
                hashset![0, 1, 2]
            );
        }

        #[test]
        fn top_folder() {
            // Given
            let p = P(vec![E("/"), E("-"), E("-"), E("-")]);
            // When
            // Then
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 0,),
                hashset![]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 1,),
                hashset![2, 3]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 2,),
                hashset![1, 3]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 3,),
                hashset![1, 2]
            );
        }

        #[test]
        fn bottom_folder() {
            // Given
            let p = P(vec![E("-"), E("-"), E("-"), E("/")]);
            // When
            // Then
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 0,),
                hashset![1, 2, 3]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 1,),
                hashset![0, 2, 3]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 2,),
                hashset![0, 1, 3]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 3,),
                hashset![0, 1, 2]
            );
        }

        #[test]
        fn top_next_folder() {
            // Given
            let p = P(vec![E("-"), E("/"), E("-"), E("-")]);
            // When
            // Then
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 0,),
                hashset![1]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 1,),
                hashset![0]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 2,),
                hashset![3]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 3,),
                hashset![2]
            );
        }

        #[test]
        fn bottom_previous_folder() {
            // Given
            let p = P(vec![E("-"), E("-"), E("/"), E("-")]);
            // When
            // Then
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 0,),
                hashset![1, 2]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 1,),
                hashset![0, 2]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 2,),
                hashset![0, 1]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 3,),
                hashset![]
            );
        }

        #[test]
        fn small_flat_top_folder() {
            // Given
            let p = P(vec![E("/"), E(r#"\"#), E("-"), E("-")]);
            // When
            // Then
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 0,),
                hashset![2, 3]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 1,),
                hashset![]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 2,),
                hashset![0, 3]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 3,),
                hashset![0, 2]
            );
        }

        #[test]
        fn large_flat_top_folder() {
            // Given
            let p = P(vec![E("/"), E("-"), E(r#"\"#), E("-")]);
            // When
            // Then
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 0,),
                hashset![3]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 1,),
                hashset![2]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 2,),
                hashset![1]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 3,),
                hashset![0]
            );
        }

        #[test]
        fn large_nested_top_folder() {
            // Given
            let p = P(vec![E("/"), E("/"), E(r#"\\"#), E("-")]);
            // When
            // Then
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 0,),
                hashset![3]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 1,),
                hashset![]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 2,),
                hashset![]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 3,),
                hashset![0]
            );
        }

        #[test]
        fn large_deeply_nested_top_folder() {
            // Given
            let p = P(vec![E("/"), E("/"), E("/"), E(r#"\\\"#), E("-")]);
            // When
            // Then
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 0,),
                hashset![4]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 1,),
                hashset![]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 2,),
                hashset![]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 3,),
                hashset![]
            );
            assert_eq!(
                test(&p, TrackExclusivity::ExclusiveWithinFolder, 4,),
                hashset![0]
            );
        }
    }

    struct TestProvider(Vec<TestEntry>);

    impl HierarchyEntryProvider for TestProvider {
        type Entry = TestEntry;

        fn find_entry_by_index(&self, index: u32) -> Option<Self::Entry> {
            self.0.get(index as usize).copied()
        }

        fn entry_count(&self) -> u32 {
            self.0.len() as _
        }
    }

    #[derive(Copy, Clone, PartialEq)]
    struct TestEntry(&'static str);

    impl HierarchyEntry for TestEntry {
        fn folder_depth_change(&self) -> i32 {
            match self.0 {
                "-" => 0,
                "/" => 1,
                r#"\"# => -1,
                r#"\\"# => -2,
                r#"\\\"# => -3,
                _ => panic!("unknown entry symbol"),
            }
        }
    }

    fn test(
        provider: &TestProvider,
        exclusivity: TrackExclusivity,
        current_index: u32,
    ) -> HashSet<u32> {
        let mut affected_indexes = HashSet::new();
        handle_exclusivity(
            provider,
            exclusivity,
            Some(current_index),
            &provider.find_entry_by_index(current_index).unwrap(),
            |i, _| {
                affected_indexes.insert(i);
            },
        );
        affected_indexes
    }
}
