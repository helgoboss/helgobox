use crate::base::*;

pub fn ok_button(id: Id, rect: Rect) -> Control {
    defpushbutton("OK", id, rect)
}

pub fn dropdown(id: Id, rect: Rect) -> Control {
    use Style::*;
    let styles = Styles(vec![
        CBS_DROPDOWNLIST,
        CBS_HASSTRINGS,
        WS_VSCROLL,
        WS_TABSTOP,
    ]);
    combobox(id, rect, styles)
}
