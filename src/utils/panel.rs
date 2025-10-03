use ratatui::{Frame, prelude::Rect};
use strum_macros::{AsRefStr, Display, EnumString};

#[derive(EnumString, AsRefStr, Display, PartialEq, Clone, Debug)]
pub enum PanelType {
    #[strum(serialize = "normal")]
    Normal,
    #[strum(serialize = "popup")]
    Popup,
}

#[derive(Debug)]
pub struct PanelInfo {
    pub panel_type: PanelType,
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl PanelInfo {
    pub fn new(panel_type: PanelType) -> Self {
        Self {
            panel_type,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        }
    }
}

pub fn panel_rect(x: u16, y: u16, width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x.saturating_add(x);
    let y = area.y.saturating_add(y);
    let width = width.min(area.width.saturating_sub(x - area.x));
    let height = height.min(area.height.saturating_sub(y - area.y));
    Rect {
        x,
        y,
        width,
        height,
    }
}

pub fn caculate_position(
    frame: &mut Frame,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
) -> (u16, u16, u16, u16) {
    let width = frame.area().width;
    let height = frame.area().height - 3;

    (
        (width as f32 * x as f32 / 100.0).round() as u16,
        (height as f32 * y as f32 / 100.0).round() as u16,
        (width as f32 * w as f32 / 100.0).round() as u16,
        (height as f32 * h as f32 / 100.0).round() as u16,
    )
}
