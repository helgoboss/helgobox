use either::Either;
use reaper_high::{
    ChangeEvent, Fx, FxAddedEvent, FxClosedEvent, FxEnabledChangedEvent, FxOpenedEvent,
    FxRemovedEvent, FxReorderedEvent, Guid, Reaper,
};
use std::collections::HashMap;
use std::iter;

/// It's a known fact that REAPER doesn't inform about changes on the monitoring FX chain via
/// control surface callback methods.
///
/// There are various ways we handle this:
///
/// - **FX parameter value changes:** By polling (only mapped parameters though, because iterating
///   over all parameters on each main loop cycle could be very resource-consuming).
/// - **FX focused:** Seems to be detected correctly already.
/// - **FX on/off, FX added/removed/reordered, FX closed/open:** We do that here by polling all
///   monitoring FX instances on each main loop cycle.
/// - **FX preset changed:** We don't do that because it takes quite a long time compared to all the
///   other checks (checked with REAPER 6.56).
#[derive(Debug, Default)]
pub struct MonitoringFxChainChangeDetector {
    items: HashMap<Guid, Item>,
}

#[derive(Debug)]
struct Item {
    fx: Fx,
    index: u32,
    enabled: bool,
    open: bool,
}

impl MonitoringFxChainChangeDetector {
    pub fn poll_for_changes(&mut self) -> Vec<ChangeEvent> {
        let new_items = gather_monitoring_fxs();
        let change_events = diff(&self.items, &new_items);
        self.items = new_items;
        change_events
    }
}

fn diff(
    previous_items: &HashMap<Guid, Item>,
    next_items: &HashMap<Guid, Item>,
) -> Vec<ChangeEvent> {
    // Removed
    let mut at_least_one_removed = false;
    let removed = previous_items.iter().filter_map(|(prev_guid, prev_item)| {
        if next_items.contains_key(prev_guid) {
            None
        } else {
            at_least_one_removed = true;
            let event = FxRemovedEvent {
                fx: prev_item.fx.clone(),
            };
            Some(ChangeEvent::FxRemoved(event))
        }
    });
    // Added
    let mut at_least_one_added = false;
    let added = next_items.iter().filter_map(|(next_guid, next_item)| {
        if previous_items.contains_key(next_guid) {
            None
        } else {
            at_least_one_added = true;
            let event = FxAddedEvent {
                fx: next_item.fx.clone(),
            };
            Some(ChangeEvent::FxAdded(event))
        }
    });
    // Reordered
    let is_reordered = previous_items.iter().any(|(prev_guid, prev_item)| {
        if let Some(next_item) = next_items.get(prev_guid) {
            next_item.index != prev_item.index
        } else {
            false
        }
    });
    let reordered = if is_reordered {
        let event = FxReorderedEvent {
            track: Reaper::get().current_project().master_track(),
        };
        Either::Left(iter::once(ChangeEvent::FxReordered(event)))
    } else {
        Either::Right(iter::empty())
    };
    // Changed
    let changed = previous_items.iter().flat_map(|(prev_guid, prev_item)| {
        if let Some(next_item) = next_items.get(prev_guid) {
            let opened_closed = if next_item.open != prev_item.open {
                let fx = next_item.fx.clone();
                let event = if next_item.open {
                    ChangeEvent::FxOpened(FxOpenedEvent { fx })
                } else {
                    ChangeEvent::FxClosed(FxClosedEvent { fx })
                };
                Some(event)
            } else {
                None
            };
            let enabled = if next_item.enabled != prev_item.enabled {
                Some(ChangeEvent::FxEnabledChanged(FxEnabledChangedEvent {
                    fx: next_item.fx.clone(),
                    new_value: next_item.enabled,
                }))
            } else {
                None
            };
            Either::Left(
                opened_closed
                    .into_iter()
                    .chain(enabled.into_iter())
                    .into_iter(),
            )
        } else {
            Either::Right(iter::empty())
        }
    });
    // Combined
    removed
        .chain(added)
        .chain(reordered)
        .chain(changed)
        .collect()
}

fn gather_monitoring_fxs() -> HashMap<Guid, Item> {
    Reaper::get()
        .monitoring_fx_chain()
        .fxs()
        .enumerate()
        .map(|(i, fx)| {
            let key = fx.guid().expect("monitoring FX no GUID");
            let value = Item {
                index: i as u32,
                enabled: fx.is_enabled(),
                open: fx.window_is_open(),
                fx,
            };
            (key, value)
        })
        .collect()
}
