use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::base::when;
use crate::infrastructure::ui::{
    bindings::root, deserialize_data_object_from_json, get_text_from_clipboard, paste_mappings,
    util, DataObject, IndependentPanelManager, MainState, MappingRowPanel,
    SharedIndependentPanelManager, SharedMainState,
};
use realearn_api::persistence::Envelope;
use reaper_high::Reaper;
use reaper_low::raw;
use rxrust::prelude::*;
use slog::debug;
use std::cmp;

use crate::application::{
    Affected, Session, SessionProp, SharedMapping, SharedSession, WeakSession,
};
use crate::domain::{Compartment, MappingId, MappingMatchedEvent};
use swell_ui::{DialogUnits, MenuBar, Pixels, Point, SharedView, View, ViewContext, Window};

#[derive(Debug)]
pub struct MappingRowsPanel {
    view: ViewContext,
    position: Point<DialogUnits>,
    session: WeakSession,
    main_state: SharedMainState,
    rows: Vec<SharedView<MappingRowPanel>>,
    panel_manager: Weak<RefCell<IndependentPanelManager>>,
    scroll_position: Cell<usize>,
}

impl MappingRowsPanel {
    pub fn new(
        session: WeakSession,
        panel_manager: Weak<RefCell<IndependentPanelManager>>,
        main_state: SharedMainState,
        position: Point<DialogUnits>,
    ) -> MappingRowsPanel {
        let row_count = 6;
        MappingRowsPanel {
            view: Default::default(),
            rows: (0..row_count)
                .map(|i| {
                    let panel = MappingRowPanel::new(
                        session.clone(),
                        i,
                        panel_manager.clone(),
                        main_state.clone(),
                        i == row_count - 1,
                    );
                    SharedView::new(panel)
                })
                .collect(),
            session,
            panel_manager,
            scroll_position: 0.into(),
            main_state,
            position,
        }
    }

    fn session(&self) -> SharedSession {
        self.session.upgrade().expect("session gone")
    }

    pub fn handle_matched_mapping(&self, event: MappingMatchedEvent) {
        if event.compartment != self.active_compartment() {
            return;
        }
        for row in &self.rows {
            if row.mapping_id() == Some(event.mapping_id) {
                row.handle_matched_mapping();
            }
        }
    }

    pub fn handle_affected(&self, affected: &Affected<SessionProp>, initiator: Option<u32>) {
        if !self.is_open() {
            return;
        }
        for row in &self.rows {
            row.handle_affected(affected, initiator);
        }
    }

    /// Also opens main panel, clears filters and switches compartment if necessary.
    ///
    /// Really tries to get mapping on top.
    pub fn force_scroll_to_mapping(&self, mapping_id: MappingId) {
        let shared_session = self.session();
        let session = shared_session.borrow();
        let (compartment, index) = match session.location_of_mapping(mapping_id) {
            None => return,
            Some(i) => i,
        };
        if !self.is_open() {
            session.show_in_floating_window();
        }
        {
            let mut main_state = self.main_state.borrow_mut();
            main_state.active_compartment.set(compartment);
            main_state.clear_all_filters_and_displayed_group();
        }
        self.scroll(index);
    }

    /// Doesn't switch compartment!
    fn ensure_mapping_is_visible(&self, compartment: Compartment, mapping_id: MappingId) {
        let index = match self.determine_index_of_mapping_in_list(compartment, mapping_id) {
            None => return,
            Some(i) => i,
        };
        let amount = self.get_scroll_amount_to_make_item_visible(index);
        self.scroll_relative(amount);
    }

    fn scroll_relative(&self, amount: isize) {
        if amount == 0 {
            return;
        }
        let new_scroll_pos = self.scroll_position.get() as isize + amount;
        if new_scroll_pos >= 0 {
            self.scroll(new_scroll_pos as usize);
        }
    }

    fn get_scroll_amount_to_make_item_visible(&self, index: usize) -> isize {
        let from_index = self.scroll_position.get();
        if index < from_index {
            return index as isize - from_index as isize;
        }
        let to_index = from_index + self.rows.len() - 1;
        if index > to_index {
            return index as isize - to_index as isize;
        }
        0
    }

    fn determine_index_of_mapping_in_list(
        &self,
        compartment: Compartment,
        mapping_id: MappingId,
    ) -> Option<usize> {
        let shared_session = self.session();
        let session = shared_session.borrow();
        let main_state = self.main_state.borrow();
        let mut mappings = Self::filtered_mappings(&session, &main_state, compartment, false);
        mappings.position(|m| m.borrow().id() == mapping_id)
    }

    pub fn edit_mapping(&self, compartment: Compartment, mapping_id: MappingId) {
        if let Some((_, m)) = self
            .session()
            .borrow()
            .find_mapping_and_index_by_id(compartment, mapping_id)
        {
            self.panel_manager().borrow_mut().edit_mapping(m);
        }
    }

    fn panel_manager(&self) -> SharedIndependentPanelManager {
        self.panel_manager.upgrade().expect("panel manager gone")
    }

    fn active_compartment(&self) -> Compartment {
        self.main_state.borrow().active_compartment.get()
    }

    fn open_mapping_rows(&self, window: Window) {
        for row in self.rows.iter() {
            row.clone().open(window);
        }
    }

    fn invalidate_scroll_info(&self) {
        let item_count = self.filtered_mapping_count();
        self.update_scroll_status_msg(item_count);
        let scroll_info = raw::SCROLLINFO {
            cbSize: std::mem::size_of::<raw::SCROLLINFO>() as _,
            fMask: raw::SIF_PAGE | raw::SIF_RANGE,
            nMin: 0,
            nMax: self.get_max_item_index(item_count) as _,
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
        let item_count = self.filtered_mapping_count();
        let fixed_pos = pos.min(self.get_max_scroll_position(item_count));
        let scroll_pos = self.scroll_position.get();
        if fixed_pos == scroll_pos {
            return false;
        }
        unsafe {
            Reaper::get().medium_reaper().low().CoolSB_SetScrollPos(
                self.view.require_window().raw() as _,
                raw::SB_VERT as _,
                fixed_pos as _,
                1,
            );
        }
        self.scroll_position.set(fixed_pos);
        self.update_scroll_status_msg(item_count);
        self.invalidate_mapping_rows();
        true
    }

    fn update_scroll_status_msg(&self, item_count: usize) {
        let from_pos = cmp::min(self.scroll_position.get() + 1, item_count);
        let to_pos = cmp::min(from_pos + self.rows.len() - 1, item_count);
        let status_msg = format!(
            "Showing mappings {} to {} of {}",
            from_pos, to_pos, item_count
        );
        self.main_state.borrow_mut().status_msg.set(status_msg);
    }

    fn get_max_item_index(&self, item_count: usize) -> usize {
        cmp::max(0, item_count as isize - 1) as usize
    }

    fn get_max_scroll_position(&self, item_count: usize) -> usize {
        cmp::max(0, item_count as isize - self.rows.len() as isize) as usize
    }

    fn filtered_mapping_count(&self) -> usize {
        let shared_session = self.session();
        let session = shared_session.borrow();
        let main_state = self.main_state.borrow();
        if !main_state.filter_and_displayed_group_is_active() {
            return session.mapping_count(self.active_compartment());
        }
        session
            .mappings(self.active_compartment())
            .filter(|m| Self::mapping_matches_filter(&session, &main_state, *m, false))
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

    pub fn filtered_mappings<'a>(
        session: &'a Session,
        main_state: &'a MainState,
        compartment: Compartment,
        ignore_group: bool,
    ) -> impl Iterator<Item = &'a SharedMapping> {
        let filter_is_active = main_state.filter_and_displayed_group_is_active();
        session.mappings(compartment).filter(move |m| {
            if filter_is_active {
                Self::mapping_matches_filter(session, main_state, *m, ignore_group)
            } else {
                true
            }
        })
    }

    /// Let mapping rows reflect the correct mappings.
    fn invalidate_mapping_rows(&self) {
        let mut row_index = 0;
        let shared_session = self.session();
        let session = shared_session.borrow();
        let main_state = self.main_state.borrow();
        let compartment = main_state.active_compartment.get();
        let filtered_mappings: Vec<_> =
            Self::filtered_mappings(&session, &main_state, compartment, false).collect();
        let scroll_pos = self.scroll_position.get();
        if scroll_pos < filtered_mappings.len() {
            for mapping in &filtered_mappings[scroll_pos..] {
                if row_index >= self.rows.len() {
                    break;
                }
                self.rows
                    .get(row_index)
                    .expect("impossible")
                    .set_mapping(Some((*mapping).clone()));
                row_index += 1;
            }
        }
        // If there are unused rows, clear them
        for i in row_index..self.rows.len() {
            self.rows.get(i).expect("impossible").set_mapping(None);
        }
        self.invalidate_empty_group_controls(
            &session,
            &main_state,
            compartment,
            filtered_mappings.len(),
        );
    }

    fn mapping_matches_filter(
        session: &Session,
        main_state: &MainState,
        mapping: &SharedMapping,
        ignore_group: bool,
    ) -> bool {
        let mapping = mapping.borrow();
        if !ignore_group {
            if let Some(group_filter) = main_state.displayed_group_for_active_compartment() {
                if !group_filter.matches(&mapping) {
                    return false;
                }
            }
        }
        if let Some(source_filter) = main_state.source_filter.get_ref() {
            let mapping_source = mapping.source_model.create_source();
            if !source_filter.matches(&mapping_source) {
                return false;
            }
        }
        if let Some(filter_target) = main_state.target_filter.get_ref() {
            if !mapping
                .with_context(session.extended_context())
                .has_target(filter_target)
            {
                return false;
            }
        }
        let search_expression = main_state.search_expression.get_ref();
        if !search_expression.is_empty()
            && !search_expression.matches(&mapping.effective_name())
            && !search_expression.matches_any_tag(mapping.tags())
            && !search_expression.matches_any_tag_in_group(&mapping, session)
        {
            return false;
        }
        true
    }

    fn invalidate_all_controls(&self) {
        self.invalidate_mapping_rows();
        self.panel_manager().borrow_mut().close_orphan_panels();
        self.invalidate_scroll_info();
    }

    fn invalidate_empty_group_controls(
        &self,
        session: &Session,
        main_state: &MainState,
        compartment: Compartment,
        displayed_mapping_count: usize,
    ) {
        let (label_text, button_text) = if displayed_mapping_count == 0 {
            use EmptyMappingListCase::*;
            match self.determine_empty_mapping_list_case(session, main_state, compartment) {
                CompartmentEmpty => (Some("There are no mappings in this compartment."), None),
                FilterSet | GroupAndFilterSetFilterIsProblem => (
                    Some("No mapping matching filter and search string."),
                    Some("Clear filter and search string".to_owned()),
                ),
                GroupSet => (
                    Some("This group is empty."),
                    Some(format!(
                        "Show {} mappings in all groups",
                        session.mapping_count(compartment)
                    )),
                ),
                GroupAndFilterSetGroupIsProblem => (
                    Some("There are matching mappings in other groups."),
                    Some("Show mappings in all groups".to_owned()),
                ),
            }
        } else {
            (None, None)
        };
        self.view
            .require_control(root::ID_GROUP_IS_EMPTY_TEXT)
            .set_text_or_hide(label_text);
        self.view
            .require_control(root::ID_DISPLAY_ALL_GROUPS_BUTTON)
            .set_text_or_hide(button_text);
    }

    fn determine_empty_mapping_list_case(
        &self,
        session: &Session,
        main_state: &MainState,
        compartment: Compartment,
    ) -> EmptyMappingListCase {
        if session.mapping_count(compartment) == 0 {
            EmptyMappingListCase::CompartmentEmpty
        } else {
            let filter_is_active = main_state.filter_is_active();
            let displayed_group = main_state.displayed_group[compartment].get();
            if !filter_is_active && displayed_group.is_some() {
                EmptyMappingListCase::GroupSet
            } else if filter_is_active && displayed_group.is_none() {
                EmptyMappingListCase::FilterSet
            } else {
                // Both filter is active and displayed group.
                // Try filter while ignoring group.
                if Self::filtered_mappings(session, main_state, compartment, true).count() == 0 {
                    EmptyMappingListCase::GroupAndFilterSetFilterIsProblem
                } else {
                    EmptyMappingListCase::GroupAndFilterSetGroupIsProblem
                }
            }
        }
    }

    fn register_listeners(self: SharedView<Self>) {
        let shared_session = self.session();
        let session = shared_session.borrow();
        let main_state = self.main_state.borrow();
        self.when(session.everything_changed(), |view, _| {
            view.invalidate_all_controls();
        });
        let main_state_clone = self.main_state.clone();
        self.when(
            session.mapping_list_changed(),
            move |view, (compartment, new_mapping_id)| {
                if compartment == main_state_clone.borrow().active_compartment.get() {
                    view.invalidate_all_controls();
                    if let Some(id) = new_mapping_id {
                        view.ensure_mapping_is_visible(compartment, id);
                    }
                }
            },
        );
        self.when(
            main_state
                .source_filter
                .changed()
                .merge(main_state.target_filter.changed())
                .merge(main_state.search_expression.changed())
                .merge(main_state.active_compartment.changed())
                .merge(main_state.displayed_group_for_any_compartment_changed())
                .merge(session.group_list_changed().map_to(())),
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

    fn fix_empty_mapping_list(&self) -> Result<(), &'static str> {
        use EmptyMappingListCase::*;
        let session = self.session();
        let session = session.borrow();
        let mut main_state = self.main_state.borrow_mut();
        let compartment = main_state.active_compartment.get();
        match self.determine_empty_mapping_list_case(&session, &main_state, compartment) {
            CompartmentEmpty => return Err("can't be fixed"),
            FilterSet | GroupAndFilterSetFilterIsProblem => main_state.clear_all_filters(),
            GroupSet | GroupAndFilterSetGroupIsProblem => {
                main_state.clear_displayed_group_for_active_compartment()
            }
        };
        Ok(())
    }

    fn open_context_menu(&self, location: Point<Pixels>) -> Result<(), &'static str> {
        let menu_bar = MenuBar::new_popup_menu();
        let pure_menu = {
            use swell_ui::menu_tree::*;
            let shared_session = self.session();
            let data_object_from_clipboard = get_text_from_clipboard()
                .and_then(|text| deserialize_data_object_from_json(&text).ok());
            let main_state = self.main_state.borrow();
            let group_id = main_state
                .displayed_group_for_active_compartment()
                .map(|f| f.group_id())
                .unwrap_or_default();
            let compartment = main_state.active_compartment.get();
            let entries = vec![{
                let desc = match data_object_from_clipboard {
                    Some(DataObject::Mapping(Envelope { value: m })) => Some((
                        format!("Paste mapping \"{}\" (insert here)", &m.name),
                        vec![*m],
                    )),
                    Some(DataObject::Mappings(Envelope { value: vec })) => {
                        Some((format!("Paste {} mappings (insert here)", vec.len()), vec))
                    }
                    _ => None,
                };
                if let Some((label, datas)) = desc {
                    item(label, move || {
                        let _ = paste_mappings(datas, shared_session, compartment, None, group_id);
                    })
                } else {
                    disabled_item("Paste")
                }
            }];
            let mut root_menu = root_menu(entries);
            root_menu.index(1);
            fill_menu(menu_bar.menu(), &root_menu);
            root_menu
        };
        let result = self
            .view
            .require_window()
            .open_popup_menu(menu_bar.menu(), location)
            .ok_or("no entry selected")?;
        pure_menu
            .find_item_by_id(result)
            .expect("selected menu item not found")
            .invoke_handler();
        Ok(())
    }

    fn when<I: Send + Sync + Clone + 'static>(
        self: &SharedView<Self>,
        event: impl LocalObservable<'static, Item = I, Err = ()> + 'static,
        reaction: impl Fn(SharedView<Self>, I) + 'static + Clone,
    ) {
        when(event.take_until(self.view.closed()))
            .with(Rc::downgrade(self))
            .do_sync(reaction);
    }
}

impl View for MappingRowsPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAPPING_ROWS_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        #[cfg(target_family = "unix")]
        unsafe {
            let reaper = Reaper::get().medium_reaper().low();
            let hwnd = window.raw() as _;
            reaper.InitializeCoolSB(hwnd);
            #[cfg(target_os = "macos")]
            if !Window::dark_mode_is_enabled() {
                // If not in dark mode, using REAPER's dark scrollbar looks weird.
                // Let's use the default bright toolbar of CoolSB. It looks ugly but
                // at least it's brighter.
                reaper.CoolSB_SetThemeIndex(hwnd, -1);
            }
        }
        window.move_to(self.position);
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

    fn control_color_static(self: SharedView<Self>, hdc: raw::HDC, _: Window) -> raw::HBRUSH {
        util::view::control_color_static_default(hdc, util::view::mapping_row_background_brush())
    }

    fn control_color_dialog(self: SharedView<Self>, hdc: raw::HDC, _: raw::HWND) -> raw::HBRUSH {
        util::view::control_color_dialog_default(hdc, util::view::mapping_row_background_brush())
    }

    fn context_menu_wanted(self: SharedView<Self>, location: Point<Pixels>) {
        let _ = self.open_context_menu(location);
    }

    #[allow(clippy::single_match)]
    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            root::ID_DISPLAY_ALL_GROUPS_BUTTON => {
                let _ = self.fix_empty_mapping_list();
            }
            _ => {}
        }
    }
}

impl Drop for MappingRowsPanel {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping mapping rows panel...");
    }
}

enum EmptyMappingListCase {
    CompartmentEmpty,
    FilterSet,
    GroupSet,
    GroupAndFilterSetFilterIsProblem,
    GroupAndFilterSetGroupIsProblem,
}
