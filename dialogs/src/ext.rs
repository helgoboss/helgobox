use crate::base::*;

impl ScopedContext<'_> {
    pub fn checkbox(&self, caption: Caption, id: Id, rect: Rect) -> Control {
        use Style::*;
        // We want to completely ignore the given checkbox height, but we want it to scale.
        let fixed_rect = self.rect_flexible(Rect { height: 8, ..rect });
        control(
            caption,
            id,
            SubControlKind::Button,
            fix_text_rect(fixed_rect),
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
    control(caption, id, SubControlKind::Button, fix_text_rect(rect)) + BS_AUTORADIOBUTTON
}

pub fn divider(id: Id, rect: Rect) -> Control {
    use Style::*;
    control("", id, SubControlKind::Static, rect) + SS_ETCHEDHORZ
}

pub fn static_text(caption: Caption, id: Id, rect: Rect) -> Control {
    control(caption, id, SubControlKind::Static, fix_text_rect(rect))
}
