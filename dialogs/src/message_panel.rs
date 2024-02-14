use crate::base::*;

pub fn create(context: ScopedContext, ids: &mut IdGenerator) -> Dialog {
    use Style::*;
    let controls = [ctext(
        "Some message",
        ids.named_id("ID_MESSAGE_TEXT"),
        context.rect(0, 0, 155, 19),
    ) + SS_CENTERIMAGE
        + SS_WORDELLIPSIS
        + NOT_WS_GROUP];
    Dialog {
        id: ids.named_id("ID_MESSAGE_PANEL"),
        caption: "ReaLearn",
        rect: context.rect(0, 0, 155, 19),
        styles: Styles(vec![
            DS_SETFONT,
            DS_MODALFRAME,
            DS_3DLOOK,
            DS_FIXEDSYS,
            DS_CENTER,
            WS_POPUP,
            WS_VISIBLE,
            WS_CAPTION,
            WS_SYSMENU,
        ]),
        ex_styles: Styles(vec![WS_EX_TOPMOST, WS_EX_WINDOWEDGE]),
        controls: controls.into_iter().collect(),
        ..context.default_dialog()
    }
}
