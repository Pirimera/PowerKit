//! Which tools the tray menu lists and in what order. Pure state; no Win32.

/// A tool that owns an entry in the tray menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayTool {
    AlwaysOn,
    AlwaysTop,
    ScreenRuler,
    ImageResizer,
    PowerStore,
    MediaRenamer,
}
impl TrayTool {
    /// Default menu order.
    pub const ALL: [Self; 6] = [
        Self::AlwaysOn,
        Self::AlwaysTop,
        Self::ScreenRuler,
        Self::ImageResizer,
        Self::PowerStore,
        Self::MediaRenamer,
    ];
    /// Stable name used in the settings file and as a UI row key.
    pub fn key(self) -> &'static str {
        match self {
            Self::AlwaysOn => "always_on",
            Self::AlwaysTop => "always_top",
            Self::ScreenRuler => "screen_ruler",
            Self::ImageResizer => "image_resizer",
            Self::PowerStore => "power_store",
            Self::MediaRenamer => "media_renamer",
        }
    }
    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| tool.key() == key)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrayItem {
    pub tool: TrayTool,
    pub shown: bool,
}
/// One user edit from the settings page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayEdit {
    Toggle(TrayTool),
    MoveUp(TrayTool),
    MoveDown(TrayTool),
}
/// Every tool exactly once, in menu order, with its visibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrayLayout {
    items: [TrayItem; 6],
}
impl Default for TrayLayout {
    fn default() -> Self {
        Self {
            items: TrayTool::ALL.map(|tool| TrayItem { tool, shown: true }),
        }
    }
}
impl TrayLayout {
    pub fn items(&self) -> &[TrayItem; 6] {
        &self.items
    }
    pub fn shown(&self, tool: TrayTool) -> bool {
        self.items
            .iter()
            .any(|item| item.tool == tool && item.shown)
    }
    fn position(&self, tool: TrayTool) -> usize {
        self.items
            .iter()
            .position(|item| item.tool == tool)
            .unwrap_or(0)
    }
    /// Applies an edit; `false` when nothing changed (moving past either end).
    pub fn apply(&mut self, edit: TrayEdit) -> bool {
        match edit {
            TrayEdit::Toggle(tool) => {
                let index = self.position(tool);
                self.items[index].shown = !self.items[index].shown;
                true
            }
            TrayEdit::MoveUp(tool) => {
                let index = self.position(tool);
                if index == 0 {
                    return false;
                }
                self.items.swap(index, index - 1);
                true
            }
            TrayEdit::MoveDown(tool) => {
                let index = self.position(tool);
                if index + 1 >= self.items.len() {
                    return false;
                }
                self.items.swap(index, index + 1);
                true
            }
        }
    }
    /// Parses `always_on:1,screen_ruler:0,...`. A repeated tool or a malformed
    /// entry is invalid. A name this build does not know is skipped, and tools the
    /// line omits follow in default order, shown, so a file written by another
    /// version keeps the order it can express.
    pub fn parse(value: &str) -> Option<Self> {
        let mut listed: Vec<TrayItem> = Vec::new();
        for entry in value.split(',') {
            let (key, flag) = entry.split_once(':')?;
            let shown = match flag {
                "1" => true,
                "0" => false,
                _ => return None,
            };
            let Some(tool) = TrayTool::from_key(key) else {
                continue;
            };
            if listed.iter().any(|item| item.tool == tool) {
                return None;
            }
            listed.push(TrayItem { tool, shown });
        }
        for tool in TrayTool::ALL {
            if !listed.iter().any(|item| item.tool == tool) {
                listed.push(TrayItem { tool, shown: true });
            }
        }
        Some(Self {
            items: listed.try_into().ok()?,
        })
    }
    pub fn append_settings(&self, out: &mut String) {
        let line: Vec<String> = self
            .items
            .iter()
            .map(|item| format!("{}:{}", item.tool.key(), u8::from(item.shown)))
            .collect();
        out.push_str(&format!("tray_tools={}\n", line.join(",")));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn order(layout: &TrayLayout) -> Vec<TrayTool> {
        layout.items().iter().map(|item| item.tool).collect()
    }
    #[test]
    fn edits_reorder_and_hide_without_losing_a_tool() {
        let mut layout = TrayLayout::default();
        assert_eq!(order(&layout), TrayTool::ALL);
        assert!(!layout.apply(TrayEdit::MoveUp(TrayTool::AlwaysOn)));
        assert!(!layout.apply(TrayEdit::MoveDown(TrayTool::MediaRenamer)));
        assert_eq!(layout, TrayLayout::default());
        assert!(layout.apply(TrayEdit::MoveUp(TrayTool::PowerStore)));
        assert!(layout.apply(TrayEdit::MoveDown(TrayTool::AlwaysOn)));
        assert_eq!(
            order(&layout),
            [
                TrayTool::AlwaysTop,
                TrayTool::AlwaysOn,
                TrayTool::ScreenRuler,
                TrayTool::PowerStore,
                TrayTool::ImageResizer,
                TrayTool::MediaRenamer,
            ]
        );
        assert!(layout.apply(TrayEdit::Toggle(TrayTool::ScreenRuler)));
        assert!(!layout.shown(TrayTool::ScreenRuler));
        assert!(layout.shown(TrayTool::PowerStore));
        assert!(layout.apply(TrayEdit::Toggle(TrayTool::ScreenRuler)));
        assert!(layout.shown(TrayTool::ScreenRuler));
    }
    #[test]
    fn settings_line_round_trips_and_tolerates_other_versions() {
        let mut layout = TrayLayout::default();
        layout.apply(TrayEdit::MoveUp(TrayTool::MediaRenamer));
        layout.apply(TrayEdit::Toggle(TrayTool::AlwaysTop));
        let mut line = String::new();
        layout.append_settings(&mut line);
        assert_eq!(
            line,
            "tray_tools=always_on:1,always_top:0,screen_ruler:1,image_resizer:1,media_renamer:1,power_store:1\n"
        );
        let value = line.trim_end().strip_prefix("tray_tools=").unwrap();
        assert_eq!(TrayLayout::parse(value), Some(layout));
        // A shorter line keeps its order; missing tools follow, shown.
        let partial = TrayLayout::parse("power_store:0,future_tool:1,always_on:1").unwrap();
        assert_eq!(
            order(&partial),
            [
                TrayTool::PowerStore,
                TrayTool::AlwaysOn,
                TrayTool::AlwaysTop,
                TrayTool::ScreenRuler,
                TrayTool::ImageResizer,
                TrayTool::MediaRenamer,
            ]
        );
        assert!(!partial.shown(TrayTool::PowerStore));
        assert!(partial.shown(TrayTool::ScreenRuler));
        for bad in [
            "",
            "always_on",
            "always_on:yes",
            "always_on:1,always_on:0",
            "always_on:1,",
        ] {
            assert_eq!(TrayLayout::parse(bad), None, "{bad}");
        }
    }
}
