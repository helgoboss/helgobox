use std::cell::{Cell, RefCell};
use std::rc::Rc;

use c_str_macro::c_str;
use helgoboss_midi::Channel;
use reaper_high::Reaper;
use reaper_low::{raw, Swell};
use rxrust::prelude::*;

use crate::core::when_async;
use crate::domain::{MappingModel, Session, SharedMapping};
use crate::infrastructure::common::bindings::root;
use crate::infrastructure::common::SharedSession;
use crate::infrastructure::ui::{
    MappingPanel, MappingPanelManager, MappingRowPanel, SharedMappingPanelManager,
};
use rx_util::UnitEvent;
use std::cmp;
use std::collections::HashMap;
use std::ops::DerefMut;
use swell_ui::{DialogUnits, Point, SharedView, View, ViewContext, Window};

pub struct MappingRowsPanel {
    view: ViewContext,
    session: SharedSession,
    rows: Vec<SharedView<MappingRowPanel>>,
    mapping_panel_manager: SharedMappingPanelManager,
    scroll_position: Cell<usize>,
}

impl MappingRowsPanel {
    pub fn new(session: SharedSession) -> MappingRowsPanel {
        let mapping_panel_manager = MappingPanelManager::new(session.clone());
        let mapping_panel_manager = Rc::new(RefCell::new(mapping_panel_manager));
        MappingRowsPanel {
            view: Default::default(),
            rows: (0..5)
                .map(|i| {
                    let panel =
                        MappingRowPanel::new(session.clone(), i, mapping_panel_manager.clone());
                    SharedView::new(panel)
                })
                .collect(),
            session,
            mapping_panel_manager,
            scroll_position: 0.into(),
        }
    }

    fn open_mapping_rows(&self, window: Window) {
        for row in self.rows.iter() {
            row.clone().open(window);
        }
    }

    fn invalidate_scroll_info(&self) {
        let scroll_info = raw::SCROLLINFO {
            cbSize: std::mem::size_of::<raw::SCROLLINFO>() as _,
            fMask: raw::SIF_PAGE | raw::SIF_RANGE,
            nMin: 0,
            nMax: cmp::max(0, self.filtered_mapping_count() as isize - 1) as _,
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
        let show = (scroll_info.nMax >= scroll_info.nPage as i32);
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

    fn scroll(&self, pos: usize) {
        let pos = pos.min(self.max_scroll_position());
        let scroll_pos = self.scroll_position.get();
        if pos == scroll_pos {
            return;
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
        self.invalidate_mapping_rows();
    }

    fn max_scroll_position(&self) -> usize {
        cmp::max(
            0,
            self.filtered_mapping_count() as isize - self.rows.len() as isize,
        ) as usize
    }

    fn filtered_mapping_count(&self) -> usize {
        // TODO
        self.session.borrow().mapping_count()
    }

    // TODO Document all those scrolling functions. It needs explanation.
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
        let mapping_count = self.session.borrow().mapping_count();
        for i in (self.scroll_position.get()..mapping_count) {
            if row_index >= self.rows.len() {
                break;
            }
            let mapping = self
                .session
                .borrow()
                .mapping_by_index(i)
                .expect("impossible");
            self.rows
                .get(row_index)
                .expect("impossible")
                .set_mapping(Some(mapping));
            row_index += 1;
        }
        // If there are unused rows, clear them
        for i in (row_index..self.rows.len()) {
            self.rows.get(i).expect("impossible").set_mapping(None);
        }
    }

    fn register_listeners(self: SharedView<Self>) {
        let session = self.session.borrow();
        self.when(session.mapping_list_changed(), |view| {
            view.invalidate_mapping_rows();
            view.mapping_panel_manager
                .borrow_mut()
                .close_orphan_panels();
            view.invalidate_scroll_info();
        });
        // TODO source/target filter
    }

    fn when(
        self: &SharedView<Self>,
        event: impl UnitEvent,
        reaction: impl Fn(SharedView<Self>) + 'static + Copy,
    ) {
        when_async(event, self.view.closed(), &self, reaction);
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
        unsafe {
            Reaper::get()
                .medium_reaper()
                .low()
                .InitializeCoolSB(window.raw() as _);
        }
        window.move_to(Point::new(DialogUnits(0), DialogUnits(78)));
        self.open_mapping_rows(window);
        self.invalidate_mapping_rows();
        self.invalidate_scroll_info();
        self.register_listeners();
        true
    }

    fn closed(self: SharedView<Self>, window: Window) {
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
                // TODO In the original ReaLearn we debounce this by 50ms. This is not yet possible
                // with rxRust.
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
