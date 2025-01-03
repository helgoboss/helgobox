use swell_ui::{Point, Rect, Window};

#[derive(Debug, Default)]
pub struct UiElementContainer {
    elements: Vec<UiElement>,
}

#[derive(Debug)]
pub struct UiElement {
    pub id: u32,
    pub rect: Rect,
    pub visible: bool,
}

impl UiElementContainer {
    pub fn fill_with_window_children(&mut self, window: Window) {
        self.fill_with_window_children_internal(window, window)
    }

    fn fill_with_window_children_internal(&mut self, ref_window: Window, current_window: Window) {
        for child in current_window.children() {
            let element = UiElement {
                id: child.resource_id(),
                rect: ref_window.screen_to_client_rect(&child.window_rect()),
                visible: true,
            };
            self.add_element(element);
            self.fill_with_window_children_internal(ref_window, child)
        }
    }

    pub fn add_element(&mut self, element: UiElement) {
        self.elements.push(element);
    }

    pub fn set_visible(&mut self, id: u32, visible: bool) {
        if let Some(el) = self.elements.iter_mut().find(|e| e.id == id) {
            el.visible = visible;
        }
    }

    pub fn hit_test(&self, point: Point<i32>) -> impl Iterator<Item = u32> + '_ {
        self.elements
            .iter()
            .filter(move |e| e.visible && e.rect.contains(point))
            .map(|e| e.id)
    }
}
