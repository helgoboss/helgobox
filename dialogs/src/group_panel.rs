use crate::base::*;
use crate::ext::*;

pub fn create(context: &mut Context) -> Dialog {
    use Style::*;
    Dialog {
        id: context.named_id("ID_GROUP_PANEL"),
        caption: "Edit group",
        rect: Rect::new(0, 0, 444, 74),
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
            context.named_id("ID_GROUP_PANEL_OK"),
            Rect::new(197, 53, 50, 14),
        )],
        ..context.default_dialog()
    }
}
