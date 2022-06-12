use crate::base::*;

impl<'a> ScopedContext<'a> {
    pub fn checkbox(&mut self, caption: Caption, id: &'static str, rect: Rect) -> Control {
        use Style::*;
        let fixed_rect = self.rect_flexible(Rect { height: 10, ..rect });
        control(
            caption,
            self.context.named_id(id),
            SubControlKind::Button,
            fixed_rect,
        ) + BS_AUTOCHECKBOX
    }
}

pub fn ok_button(id: Id, rect: Rect) -> Control {
    defpushbutton("OK", id, rect)
}

pub fn dropdown(id: Id, rect: Rect) -> Control {
    use Style::*;
    combobox(id, rect) + CBS_DROPDOWNLIST + CBS_HASSTRINGS
}

pub fn slider(id: Id, rect: Rect) -> Control {
    use Style::*;
    control("", id, SubControlKind::msctls_trackbar32, rect) + TBS_BOTH + TBS_NOTICKS
}

pub fn radio_button(caption: Caption, id: Id, rect: Rect) -> Control {
    use Style::*;
    control(caption, id, SubControlKind::Button, rect) + BS_AUTORADIOBUTTON
}

pub fn divider(id: Id, rect: Rect) -> Control {
    use Style::*;
    control("", id, SubControlKind::Static, rect) + SS_ETCHEDHORZ
}

pub fn static_text(caption: Caption, id: Id, rect: Rect) -> Control {
    control(caption, id, SubControlKind::Static, rect)
}