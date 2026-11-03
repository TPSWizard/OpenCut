mod badge;
mod button;
#[allow(dead_code)]
mod context_menu;
#[allow(dead_code)]
mod label;
mod resizable;
mod separator;
mod text_input;

#[allow(unused_imports)]
pub(crate) use badge::{Badge, BadgeVariant};
pub(crate) use button::{Button, ButtonSize, ButtonVariant};
#[allow(unused_imports)]
pub(crate) use context_menu::{
    ContextMenu, ContextMenuItem, ContextMenuItemVariant, context_menu_trigger,
};
#[allow(unused_imports)]
pub(crate) use label::Label;
pub(crate) use resizable::{Orientation, ResizablePanelGroup};
pub(crate) use separator::Separator;
pub(crate) use text_input::{TextInput, bind_keys as bind_text_input_keys};

/// An element id made of a prefix and a dynamic key (GPUI only derives ids
/// from strings and integers).
pub(crate) fn eid(prefix: &str, key: &str) -> gpui::ElementId {
    gpui::ElementId::Name(format!("{prefix}:{key}").into())
}
