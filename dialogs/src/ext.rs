use crate::base::*;

pub fn ok_button(id: Id, rect: Rect) -> Control {
    defpushbutton("OK", id, rect)
}

pub fn simple_dropdown(id: Id, rect: Rect) -> Control {
    dropdown(id, rect, Styles::default())
}

pub fn dropdown(id: Id, rect: Rect, additional_styles: Styles) -> Control {
    use Style::*;
    let mut styles = Styles(vec![
        CBS_DROPDOWNLIST,
        CBS_HASSTRINGS,
        WS_VSCROLL,
        WS_TABSTOP,
    ]);
    styles.0.extend(additional_styles.0.into_iter());
    combobox(id, rect, styles)
}

pub fn checkbox(caption: Caption, id: Id, rect: Rect) -> Control {
    use Style::*;
    let styles = Styles(vec![BS_AUTOCHECKBOX, WS_TABSTOP]);
    control(caption, id, SubControlKind::Button, styles, rect)
}

pub fn radio_button(caption: Caption, id: Id, rect: Rect) -> Control {
    use Style::*;
    let styles = Styles(vec![BS_AUTORADIOBUTTON, WS_TABSTOP]);
    control(caption, id, SubControlKind::Button, styles, rect)
}

pub fn divider(id: Id, rect: Rect) -> Control {
    use Style::*;
    let styles = Styles(vec![SS_ETCHEDHORZ]);
    control("", id, SubControlKind::Static, styles, rect)
}
