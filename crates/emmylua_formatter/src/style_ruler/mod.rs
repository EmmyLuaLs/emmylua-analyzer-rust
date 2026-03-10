mod basic_space;

use crate::{format::LuaFormatter, styles::LuaCodeStyle};

#[allow(unused)]
pub fn apply_styles(formatter: &mut LuaFormatter, styles: &LuaCodeStyle) {
    apply_style::<basic_space::BasicSpaceRuler>(formatter, styles);
}

pub trait StyleRuler {
    /// Apply the style rules to the formatter
    fn apply_style(formatter: &mut LuaFormatter, styles: &LuaCodeStyle);
}

pub fn apply_style<T: StyleRuler>(formatter: &mut LuaFormatter, styles: &LuaCodeStyle) {
    T::apply_style(formatter, styles)
}
