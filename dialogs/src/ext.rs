use crate::base::*;

pub fn ok_button(id: Id, rect: Rect) -> Control {
    defpushbutton("OK", id, rect)
}

pub fn dropdown(id: Id, rect: Rect) -> Control {
    use Style::*;
    combobox(id, rect) + CBS_DROPDOWNLIST + CBS_HASSTRINGS
}

pub fn checkbox(caption: Caption, id: Id, rect: Rect) -> Control {
    use Style::*;
    control(caption, id, SubControlKind::Button, rect) + BS_AUTOCHECKBOX
}

pub fn radio_button(caption: Caption, id: Id, rect: Rect) -> Control {
    use Style::*;
    control(caption, id, SubControlKind::Button, rect) + BS_AUTORADIOBUTTON
}

pub fn divider(id: Id, rect: Rect) -> Control {
    use Style::*;
    control("", id, SubControlKind::Static, rect) + SS_ETCHEDHORZ
}
