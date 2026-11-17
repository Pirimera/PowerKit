//! Right-side overlay drawers for MediaRenamer: item details, rename history with
//! undo, and the tool's persistent defaults.
use super::super::media_renamer::{
    Action, BatchView, Busy, DetailsView, DrawerView, MemberView, ProbeView,
};
use super::super::settings_ui;
use super::store_page::{button_row, sized_icon};
use super::{Message, Shell};
use crate::locale::RenameStrings;
use crate::media_renamer::history::{Kind as BatchKind, State as BatchState};
use windows_reactor::*;

const DRAWER_WIDTH: f64 = 416.0;
const GLYPH_CLOSE: &str = "\u{E711}";

impl Shell {
    pub(super) fn media_renamer_drawer_overlay(&self, context: &ViewContext<Self>) -> Option<View> {
        let s = &self.snapshot.media_renamer;
        let r = self.t().rename;
        let (title, subtitle, body) = match &s.drawer {
            DrawerView::Hidden => return None,
            DrawerView::Details(details) => (
                details.title.clone(),
                format!("{}: {}", r.folder, details.folder),
                details_body(r, details),
            ),
            DrawerView::History(batches) => (
                r.history.to_string(),
                String::new(),
                self.media_renamer_history_body(context, batches),
            ),
            DrawerView::Settings => (
                r.settings_title.to_string(),
                String::new(),
                self.media_renamer_settings_body(context),
            ),
        };
        let close = || context.message(Message::MediaRenamer(Action::CloseDrawer));
        let scrim = Button::new()
            .grid_column(0)
            .style(ButtonStyle::Subtle)
            .opacity(0.28)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .vertical_alignment(VerticalAlignment::Stretch)
            .on_click(close())
            .content("");
        let mut labels = vec![settings_ui::heading(&title, 18.0).into()];
        if !subtitle.is_empty() {
            labels.push(settings_ui::caption(&subtitle));
        }
        let header = Grid::new()
            .grid_row(0)
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(12.0)
            .margin(Thickness::new(20.0, 16.0, 14.0, 12.0))
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((settings_ui::stack(labels, 2.0),)),
                Button::new()
                    .grid_column(1)
                    .width(34.0)
                    .height(32.0)
                    .style(ButtonStyle::Subtle)
                    .vertical_alignment(VerticalAlignment::Top)
                    .on_click(close())
                    .content(sized_icon(GLYPH_CLOSE, 12.0))
                    .tooltip(r.close),
            ));
        let divider = Border::new()
            .grid_row(1)
            .height(1.0)
            .background(ThemeBrush::CardStroke);
        let scroll = ScrollViewer::new()
            .grid_row(2)
            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
            .content(
                StackPanel::new()
                    .margin(Thickness::new(20.0, 14.0, 20.0, 24.0))
                    .children((settings_ui::stack(body, 10.0),)),
            );
        let panel = Border::new()
            .grid_column(1)
            .width(DRAWER_WIDTH)
            .vertical_alignment(VerticalAlignment::Stretch)
            .background(ThemeBrush::SolidBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::new(1.0, 0.0, 0.0, 0.0))
            .content(
                Grid::new()
                    .rows([GridLength::Auto, GridLength::Auto, GridLength::STAR])
                    .children((header, divider, scroll)),
            );
        Some(
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .children((scrim, panel)),
        )
    }

    fn media_renamer_history_body(
        &self,
        context: &ViewContext<Self>,
        batches: &[BatchView],
    ) -> Vec<View> {
        let s = &self.snapshot.media_renamer;
        let r = self.t().rename;
        let idle = s.busy == Busy::Idle;
        if batches.is_empty() {
            return vec![settings_ui::caption(r.history_empty)];
        }
        let mut rows: Vec<View> = batches
            .iter()
            .map(|batch| batch_card(context, r, batch, idle))
            .collect();
        rows.push(button_row(vec![
            Button::new()
                .is_enabled(idle)
                .on_click(context.message(Message::MediaRenamer(Action::ClearHistory)))
                .content(r.clear_history),
        ]));
        rows
    }

    fn media_renamer_settings_body(&self, context: &ViewContext<Self>) -> Vec<View> {
        let s = &self.snapshot.media_renamer;
        let r = self.t().rename;
        let message = |action: Action| context.message(Message::MediaRenamer(action));
        vec![
            settings_ui::section(r.defaults_section),
            settings_ui::described_card(
                r.default_template,
                settings_ui::caption(&s.default_template),
                Button::new()
                    .is_enabled(s.template_error.is_none() && s.template != s.default_template)
                    .on_click(message(Action::SaveDefaultTemplate))
                    .content(r.save_default),
            ),
            settings_ui::card(
                r.default_recurse,
                settings_ui::toggle_switch(
                    ToggleSwitch::new().is_on(s.default_recurse).on_toggled(
                        context.callback(|on| Message::MediaRenamer(Action::SetDefaultRecurse(on))),
                    ),
                ),
            ),
            settings_ui::card(
                r.default_mtime_fallback,
                settings_ui::toggle_switch(
                    ToggleSwitch::new()
                        .is_on(s.default_mtime_fallback)
                        .on_toggled(context.callback(|on| {
                            Message::MediaRenamer(Action::SetDefaultMtimeFallback(on))
                        })),
                ),
            ),
            settings_ui::described_card(
                r.lowercase_ext,
                settings_ui::caption(r.lowercase_ext_hint),
                settings_ui::toggle_switch(ToggleSwitch::new().is_on(s.lowercase_ext).on_toggled(
                    context.callback(|on| Message::MediaRenamer(Action::SetLowercaseExt(on))),
                )),
            ),
            settings_ui::section(r.explorer_section),
            settings_ui::caption(r.explorer_hint),
        ]
    }
}

fn details_body(r: &RenameStrings, details: &DetailsView) -> Vec<View> {
    let dash = |value: &str| -> String {
        if value.is_empty() {
            "—".into()
        } else {
            value.to_string()
        }
    };
    let mut state = r.state(details.state);
    if details.collided {
        state = format!("{state} · {}", r.collided);
    }
    let camera = [details.make.as_str(), details.model.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut facts = vec![
        settings_ui::caption(&format!("{}: {state}", r.col_state)),
        settings_ui::caption(&format!("{}: {}", r.capture_time, dash(&details.time))),
    ];
    if let Some(offset) = &details.offset {
        facts.push(settings_ui::caption(&format!("{}: {offset}", r.offset)));
    }
    facts.push(settings_ui::caption(&format!(
        "{}: {}",
        r.source,
        details.source.map(|source| r.source(source)).unwrap_or("—")
    )));
    if !camera.is_empty() {
        facts.push(settings_ui::caption(&format!("{}: {camera}", r.camera)));
    }
    let mut rows = vec![
        settings_ui::surface(settings_ui::stack(facts, 4.0)),
        settings_ui::section(&format!("{} ({})", r.files, details.members.len())),
    ];
    rows.extend(details.members.iter().map(|member| member_card(r, member)));
    rows
}

fn member_card(r: &RenameStrings, member: &MemberView) -> View {
    let role = if member.primary {
        r.primary
    } else if member.companion {
        r.companion
    } else {
        ""
    };
    let probe = match member.probe {
        ProbeView::Pending => r.state_pending.to_string(),
        ProbeView::Read => String::new(),
        ProbeView::OnlineOnly => r.state_online_only.to_string(),
        ProbeView::Unreadable => r.state_unreadable.to_string(),
        ProbeView::CodecMissing(codec) => r.state_codec_missing.replace("{codec}", codec),
    };
    let mut parts = vec![
        TextBlock::new()
            .text(member.name.as_str())
            .font_weight(FontWeight::SEMI_BOLD)
            .text_wrapping(TextWrapping::Wrap)
            .into(),
    ];
    if !member.new_name.is_empty() && member.new_name != member.name {
        parts.push(settings_ui::caption(&format!("→ {}", member.new_name)));
    }
    let meta = [role, probe.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    if !meta.is_empty() {
        parts.push(settings_ui::caption(&meta));
    }
    settings_ui::surface(settings_ui::stack(parts, 3.0))
}

fn batch_card(
    context: &ViewContext<Shell>,
    r: &RenameStrings,
    batch: &BatchView,
    idle: bool,
) -> View {
    let kind = match batch.kind {
        BatchKind::Rename => r.batch_rename,
        BatchKind::Undo => r.batch_undo,
    };
    let state = match batch.state {
        BatchState::Pending => r.batch_pending,
        BatchState::Done => r.batch_done,
        BatchState::Interrupted => r.batch_interrupted,
        BatchState::Undone => r.batch_undone,
    };
    let mut summary = format!(
        "{} · {state} · {}",
        batch.at,
        RenameStrings::count(r.batch_files, batch.files)
    );
    if batch.failed > 0 {
        summary.push_str(" · ");
        summary.push_str(&RenameStrings::count(r.batch_failed, batch.failed as usize));
    }
    let mut parts = vec![
        TextBlock::new()
            .text(kind)
            .font_weight(FontWeight::SEMI_BOLD)
            .into(),
        settings_ui::caption(&summary),
    ];
    parts.extend(
        batch
            .sample
            .iter()
            .map(|(from, to)| settings_ui::caption(&format!("{from} → {to}"))),
    );
    if batch.undoable {
        let id = batch.id;
        parts.push(button_row(vec![
            Button::new()
                .is_enabled(idle)
                .on_click(context.message(Message::MediaRenamer(Action::Undo(id))))
                .content(r.undo),
        ]));
    }
    settings_ui::surface(settings_ui::stack(parts, 4.0))
}
