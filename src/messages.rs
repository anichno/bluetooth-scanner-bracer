#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LightControls {
    BrightnessIncrease,
    BrightnessDecrease,
    ModeChange(DisplaySortMode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisplaySortMode {
    Sticky,
    Ordered,
}
