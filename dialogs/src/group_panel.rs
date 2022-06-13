use crate::base::*;
use crate::ext::*;

pub fn create(context: ScopedContext, ids: &mut IdGenerator) -> Dialog {
    use Style::*;
    Dialog {
        id: ids.named_id("ID_GROUP_PANEL"),
        caption: "Edit group",
        rect: context.rect(0, 0, 444, 74),
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
        controls: vec![ok_button(
            ids.named_id("ID_GROUP_PANEL_OK"),
            context.rect(197, 53, 50, 14),
        )],
        ..context.default_dialog()
    }
}
