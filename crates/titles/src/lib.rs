//! Titles (row 98): where a credit roll or crawl puts its text as the clip
//! plays (`roll`) and the text template library with its placement on a
//! canvas (`templates`). Pure maths and data, shared by the editor and,
//! through WASM, the web editor.

mod roll;
mod templates;

pub use roll::{
    CreditRoll, ROLL_SPEED_MAX, ROLL_SPEED_MIN, RollBox, RollDirection, credit_roll_from, credit_roll_position,
    credit_roll_seconds, credit_roll_span,
};
pub use templates::{
    LIBRARY_JSON, PlacedTextItem, TextTemplate, TextTemplateCategory, TextTemplateItem, place_text_template, text_template,
    text_template_categories, text_template_font_scale, text_templates,
};
