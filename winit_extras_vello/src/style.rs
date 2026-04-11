//! Menu styling configuration.

/// Visual style for vello-rendered context menus.
#[derive(Debug, Clone)]
pub struct MenuStyle {
    pub background: [u8; 4],
    pub text_color: [u8; 4],
    pub hover_background: [u8; 4],
    pub separator_color: [u8; 4],
    pub disabled_text_color: [u8; 4],
    pub check_color: [u8; 4],
    pub item_height: u32,
    pub separator_height: u32,
    pub padding_x: u32,
    pub padding_y: u32,
    pub font_size: u32,
    pub min_width: u32,
}

impl MenuStyle {
    pub fn light() -> Self {
        Self {
            background: [245, 245, 245, 255],
            text_color: [30, 30, 30, 255],
            hover_background: [0, 120, 212, 255],
            separator_color: [200, 200, 200, 255],
            disabled_text_color: [160, 160, 160, 255],
            check_color: [0, 120, 212, 255],
            item_height: 32,
            separator_height: 9,
            padding_x: 28,
            padding_y: 6,
            font_size: 18,
            min_width: 200,
        }
    }

    pub fn dark() -> Self {
        Self {
            background: [43, 43, 43, 255],
            text_color: [230, 230, 230, 255],
            hover_background: [65, 65, 65, 255],
            separator_color: [80, 80, 80, 255],
            disabled_text_color: [120, 120, 120, 255],
            check_color: [96, 165, 250, 255],
            item_height: 32,
            separator_height: 9,
            padding_x: 28,
            padding_y: 6,
            font_size: 18,
            min_width: 200,
        }
    }
}

impl Default for MenuStyle {
    fn default() -> Self {
        Self::light()
    }
}
