use std::cell::{Cell, RefCell};
use std::rc::Rc;

use reaper_high::Reaper;
use reaper_low::raw;

use crate::core::when;
use crate::infrastructure::ui::{
    bindings::root, MainPanel, MappingPanelManager, MappingRowPanel, SharedMainState,
    SharedMappingPanelManager,
};
use rx_util::{SharedItemEvent, SharedPayload};
use slog::debug;
use std::cmp;

use crate::application::{MappingModel, SharedMapping, SharedSession, WeakSession};
use crate::domain::{CompoundMappingTarget, MappingCompartment};
use swell_ui::{DialogUnits, Point, SharedView, View, ViewContext, WeakView, Window};

#[derive(Debug)]
pub struct MappingRowsPanel {
    view: ViewContext,
    session: WeakSession,
    main_state: SharedMainState,
    rows: Vec<SharedView<MappingRowPanel>>,
    mapping_panel_manager: SharedMappingPanelManager,
    scroll_position: Cell<usize>,
}

impl MappingRowsPanel {
    pub fn new(
        session: WeakSession,
        main_panel: WeakView<MainPanel>,
        main_state: SharedMainState,
    ) -> MappingRowsPanel {
        let mapping_panel_manager = MappingPanelManager::new(session.clone(), main_panel);
        let mapping_panel_manager = Rc::new(RefCell::new(mapping_panel_manager));
        MappingRowsPanel {
            view: Default::default(),
            rows: (0..6)
                .map(|i| {
                    let panel = MappingRowPanel::new(
                        session.clone(),
                        i,
                        mapping_panel_manager.clone(),
                        main_state.clone(),
                    );
                    SharedView::new(panel)
                })
                .collect(),
            session,
            mapping_panel_manager,
            scroll_position: 0.into(),
            main_state,
        }
    }

    fn session(&self) -> SharedSession {
        self.session.upgrade().expect("session gone")
    }

    pub fn scroll_to_mapping(&self, mapping: *const MappingModel) {
        let shared_session = self.session();
        let session = shared_session.borrow();
        let index = match session.index_of_mapping(self.active_compartment(), mapping) {
            None => return,
            Some(i) => i,
        };
        self.main_state.borrow_mut().clear_filters();
        if !self.is_open() {
            session.show_in_floating_window();
        }
        self.scroll(index);
    }

    pub fn edit_mapping(&self, mapping: *const MappingModel) {
        if let Some(m) = self
            .session()
            .borrow()
            .find_mapping_by_address(self.active_compartment(), mapping)
        {
            self.mapping_panel_manager.borrow_mut().edit_mapping(m);
        }
    }

    fn active_compartment(&self) -> MappingCompartment {
        self.main_state.borrow().active_compartment.get()
    }

    fn open_mapping_rows(&self, window: Window) {
        for row in self.rows.iter() {
            row.clone().open(window);
        }
    }

    fn invalidate_scroll_info(&self) {
        let count = self.filtered_mapping_count();
        self.main_state
            .borrow_mut()
            .right_status_msg
            .set(count.to_string());
        let scroll_info = raw::SCROLLINFO {
            cbSize: std::mem::size_of::<raw::SCROLLINFO>() as _,
            fMask: raw::SIF_PAGE | raw::SIF_RANGE,
            nMin: 0,
            nMax: cmp::max(0, count as isize - 1) as _,
            nPage: self.rows.len() as _,
            nPos: 0,
            nTrackPos: 0,
        };
        unsafe {
            Reaper::get().medium_reaper().low().CoolSB_SetScrollInfo(
                self.view.require_window().raw() as _,
                raw::SB_VERT as _,
                &scroll_info as *const _ as *mut _,
                1,
            );
        }
        self.adjust_scrolling(&scroll_info);
        self.show_or_hide_scroll_bar(&scroll_info);
    }

    fn show_or_hide_scroll_bar(&self, scroll_info: &raw::SCROLLINFO) {
        let show = scroll_info.nMax >= scroll_info.nPage as i32;
        unsafe {
            Reaper::get().medium_reaper().low().CoolSB_ShowScrollBar(
                self.view.require_window().raw() as _,
                raw::SB_VERT as _,
                show as _,
            );
        }
    }

    fn adjust_scrolling(&self, scroll_info: &raw::SCROLLINFO) {
        let max_scroll_pos =
            cmp::max(0, (scroll_info.nMax + 1) - scroll_info.nPage as i32) as usize;
        if max_scroll_pos == 0 {
            self.scroll(0);
            return;
        }
        let scroll_pos = self.scroll_position.get();
        if scroll_pos > max_scroll_pos || (scroll_pos == max_scroll_pos - 1 && scroll_pos > 0) {
            self.scroll(max_scroll_pos);
        }
    }

    fn scroll(&self, pos: usize) -> bool {
        let pos = pos.min(self.max_scroll_position());
        let scroll_pos = self.scroll_position.get();
        if pos == scroll_pos {
            return false;
        }
        unsafe {
            Reaper::get().medium_reaper().low().CoolSB_SetScrollPos(
                self.view.require_window().raw() as _,
                raw::SB_VERT as _,
                pos as _,
                1,
            );
        }
        self.scroll_position.set(pos);
        self.main_state
            .borrow_mut()
            .left_status_msg
            .set(pos.to_string());
        self.invalidate_mapping_rows();
        true
    }

    fn max_scroll_position(&self) -> usize {
        cmp::max(
            0,
            self.filtered_mapping_count() as isize - self.rows.len() as isize,
        ) as usize
    }

    fn filtered_mapping_count(&self) -> usize {
        let shared_session = self.session();
        let session = shared_session.borrow();
        let main_state = self.main_state.borrow();
        if main_state.source_filter.get_ref().is_none()
            && main_state.target_filter.get_ref().is_none()
            && main_state.search_expression.get_ref().trim().is_empty()
        {
            return session.mapping_count(self.active_compartment());
        }
        session
            .mappings(self.active_compartment())
            .filter(|m| self.mapping_matches_filter(*m))
            .count()
    }

    // TODO-low Document all those scrolling functions. It needs explanation.
    fn scroll_pos(&self, code: u32) -> Option<usize> {
        let mut si = raw::SCROLLINFO {
            cbSize: std::mem::size_of::<raw::SCROLLINFO>() as _,
            fMask: raw::SIF_PAGE | raw::SIF_POS | raw::SIF_RANGE | raw::SIF_TRACKPOS,
            nMin: 0,
            nMax: 0,
            nPage: 0,
            nPos: 0,
            nTrackPos: 0,
        };
        unsafe {
            Reaper::get().medium_reaper().low().CoolSB_GetScrollInfo(
                self.view.require_window().raw() as _,
                raw::SB_VERT as _,
                &mut si as *mut raw::SCROLLINFO as _,
            );
        }
        let min_pos = si.nMin;
        let max_pos = si.nMax - (si.nPage as i32 - 1);
        let result = match code {
            raw::SB_LINEUP => cmp::max(si.nPos - 1, min_pos),
            raw::SB_LINEDOWN => cmp::min(si.nPos + 1, max_pos),
            raw::SB_PAGEUP => cmp::max(si.nPos - si.nPage as i32, min_pos),
            raw::SB_PAGEDOWN => cmp::min(si.nPos + si.nPage as i32, max_pos),
            raw::SB_THUMBTRACK => si.nTrackPos,
            raw::SB_TOP => min_pos,
            raw::SB_BOTTOM => max_pos,
            _ => return None,
        };
        Some(result as _)
    }

    /// Let mapping rows reflect the correct mappings.
    fn invalidate_mapping_rows(&self) {
        let mut row_index = 0;
        let mapping_count = self
            .session()
            .borrow()
            .mapping_count(self.active_compartment());
        for i in self.scroll_position.get()..mapping_count {
            if row_index >= self.rows.len() {
                break;
            }
            let shared_session = self.session();
            let session = shared_session.borrow();
            let mapping = session
                .find_mapping_by_index(self.active_compartment(), i)
                .expect("impossible");
            if !self.mapping_matches_filter(mapping) {
                continue;
            }
            self.rows
                .get(row_index)
                .expect("impossible")
                .set_mapping(Some(mapping.clone()));
            row_index += 1;
        }
        // If there are unused rows, clear them
        for i in row_index..self.rows.len() {
            self.rows.get(i).expect("impossible").set_mapping(None);
        }
    }

    fn mapping_matches_filter(&self, mapping: &SharedMapping) -> bool {
        let main_state = self.main_state.borrow();
        if let Some(filter_source) = main_state.source_filter.get_ref() {
            let mapping_source = mapping.borrow().source_model.create_source();
            if mapping_source != *filter_source {
                return false;
            }
        }
        if let Some(filter_target) = main_state.target_filter.get_ref() {
            let mapping_target = match mapping
                .borrow()
                .target_model
                .with_context(self.session().borrow().context())
                .create_target()
            {
                Ok(CompoundMappingTarget::Reaper(t)) => t,
                _ => return false,
            };
            if mapping_target != *filter_target {
                return false;
            }
        }
        let search_expression = main_state.search_expression.get_ref().trim().to_lowercase();
        if !search_expression.is_empty()
            && !mapping
                .borrow()
                .name
                .get_ref()
                .to_lowercase()
                .contains(&search_expression)
        {
            return false;
        }
        true
    }

    fn invalidate_all_controls(&self) {
        self.invalidate_mapping_rows();
        self.mapping_panel_manager
            .borrow_mut()
            .close_orphan_panels();
        self.invalidate_scroll_info();
    }

    fn register_listeners(self: SharedView<Self>) {
        let shared_session = self.session();
        let session = shared_session.borrow();
        let main_state = self.main_state.borrow();
        self.when(session.everything_changed(), |view, _| {
            view.invalidate_all_controls();
        });
        let main_state_clone = self.main_state.clone();
        self.when(session.mapping_list_changed(), move |view, compartment| {
            if compartment == main_state_clone.borrow().active_compartment.get() {
                view.invalidate_all_controls();
            }
        });
        self.when(
            main_state
                .source_filter
                .changed()
                .merge(main_state.target_filter.changed())
                .merge(main_state.search_expression.changed())
                .merge(main_state.active_compartment.changed()),
            |view, _| {
                if !view.scroll(0) {
                    // No scrolling was necessary. But that also means, the rows were not
                    // invalidated. Do it now!
                    view.invalidate_mapping_rows();
                }
                view.invalidate_scroll_info();
            },
        );
    }

    fn when<I: SharedPayload>(
        self: &SharedView<Self>,
        event: impl SharedItemEvent<I>,
        reaction: impl Fn(SharedView<Self>, I) + 'static + Clone,
    ) {
        when(event.take_until(self.view.closed()))
            .with(Rc::downgrade(self))
            .do_sync(move |panel, item| reaction(panel, item));
    }
}

impl View for MappingRowsPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAPPING_ROWS_DIALOG
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        #[cfg(target_family = "unix")]
        unsafe {
            Reaper::get()
                .medium_reaper()
                .low()
                .InitializeCoolSB(window.raw() as _);
        }
        window.move_to(Point::new(DialogUnits(0), DialogUnits(100)));
        self.open_mapping_rows(window);
        self.invalidate_mapping_rows();
        self.invalidate_scroll_info();
        self.register_listeners();
        true
    }

    #[allow(unused_variables)]
    fn closed(self: SharedView<Self>, window: Window) {
        #[cfg(target_family = "unix")]
        unsafe {
            Reaper::get()
                .medium_reaper()
                .low()
                .UninitializeCoolSB(window.raw() as _);
        }
    }

    fn scrolled_vertically(self: SharedView<Self>, code: u32) -> bool {
        match self.scroll_pos(code) {
            None => false,
            Some(scroll_pos) => {
                // TODO-low In the original ReaLearn we debounce this by 50ms. This is not yet
                // possible with rxRust. It's possible to implement this without Rx though. But
                // right now it doesn't seem to be even necessary. We could also just update
                // a few controls when thumb tracking, not everything. Probably even better!
                self.scroll(scroll_pos);
                true
            }
        }
    }

    fn mouse_wheel_turned(self: SharedView<Self>, distance: i32) -> bool {
        let code = if distance < 0 {
            raw::SB_LINEDOWN
        } else {
            raw::SB_LINEUP
        };
        let new_scroll_pos = self.scroll_pos(code).expect("impossible");
        self.scroll(new_scroll_pos);
        true
    }
}

impl Drop for MappingRowsPanel {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping mapping rows panel...");
    }
}
