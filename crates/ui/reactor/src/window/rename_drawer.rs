//! Right-side overlay drawers for MediaRenamer: item details (with full metadata),
//! rename history with undo, and the tool's persistent defaults.
use super::super::settings_ui;
use super::rename_page::{media_kind_tile, state_pill_view};
use super::view_helpers::{
    DRAWER_ICON_BOX, DrawerChrome, PillTone, compact_empty_row, drawer_icon_badge, drawer_section,
    drawer_shell, empty_card, fact_row, grouped_setting_row, grouped_surface, hairline, icon_label,
    inset_box, inset_notice, sized_icon, status_pill,
};
use super::{Message, Shell};
use powerkit_app::controller::media_renamer::{
    Action, BatchView, Busy, DetailsView, DrawerView, MemberView, MetadataView, ProbeView,
};
use powerkit_core::locale::RenameStrings;
use powerkit_core::media_renamer::{
    exif::ExifTag,
    history::{Kind as BatchKind, State as BatchState},
};
use windows_reactor::*;

/// Width of the tag-name column in the metadata list.
const TAG_NAME_WIDTH: f64 = 140.0;
const MEMBER_TILE_SIZE: f64 = 30.0;

const GLYPH_HISTORY: &str = "\u{E81C}";
const GLYPH_SETTINGS: &str = "\u{E713}";
const GLYPH_RENAME: &str = "\u{E8AC}";
const GLYPH_UNDO: &str = "\u{E7A7}";
const GLYPH_DELETE: &str = "\u{E74D}";
const GLYPH_FOLDER: &str = "\u{E8B7}";
const GLYPH_INFO: &str = "\u{E946}";
const GLYPH_SAVE: &str = "\u{E74E}";
const GLYPH_CHECK: &str = "\u{E73E}";
const GLYPH_ERROR: &str = "\u{EA39}";
const GLYPH_WARNING: &str = "\u{E7BA}";

impl Shell {
    pub(super) fn media_renamer_drawer_overlay(&self, context: &ViewContext<Self>) -> Option<View> {
        let s = &self.snapshot.media_renamer;
        let r = self.t().rename;

        let (leading, title, title_badge, subtitle, sticky_bar, body) = match &s.drawer {
            DrawerView::Hidden => return None,
            DrawerView::Details(details) => (
                Some(media_kind_tile(details.kind, DRAWER_ICON_BOX)),
                details.title.clone(),
                None,
                format!("{}: {}", r.folder, details.folder),
                None,
                details_body(r, details),
            ),
            DrawerView::History(batches) => {
                let count_badge = (!batches.is_empty()).then(|| batches.len().to_string());
                let sticky = self.rename_history_sticky_bar(context, batches);
                (
                    Some(drawer_icon_badge(GLYPH_HISTORY)),
                    r.history.to_string(),
                    count_badge,
                    String::new(),
                    sticky,
                    self.media_renamer_history_body(context, batches),
                )
            }
            DrawerView::Settings => (
                Some(drawer_icon_badge(GLYPH_SETTINGS)),
                r.settings_title.to_string(),
                None,
                String::new(),
                None,
                self.media_renamer_settings_body(context),
            ),
        };

        Some(drawer_shell(
            DrawerChrome {
                leading,
                title,
                title_badge,
                subtitle,
                sticky_bar,
                body,
                close_tooltip: r.close,
            },
            || context.message(Message::MediaRenamer(Action::CloseDrawer)),
        ))
    }

    fn rename_history_sticky_bar(
        &self,
        context: &ViewContext<Self>,
        batches: &[BatchView],
    ) -> Option<View> {
        if batches.is_empty() {
            return None;
        }
        let s = &self.snapshot.media_renamer;
        let r = self.t().rename;
        let idle = s.busy == Busy::Idle;
        let (done, interrupted, undone) = batch_counts(batches);

        let mut pills: Vec<View> = Vec::new();
        if done > 0 {
            pills.push(status_pill(
                Some(GLYPH_CHECK),
                &done.to_string(),
                PillTone::Success,
            ));
        }
        if interrupted > 0 {
            pills.push(status_pill(
                Some(GLYPH_WARNING),
                &interrupted.to_string(),
                PillTone::Warning,
            ));
        }
        if undone > 0 {
            pills.push(status_pill(
                Some(GLYPH_UNDO),
                &undone.to_string(),
                PillTone::Muted,
            ));
        }
        let summary = StackPanel::new()
            .grid_column(0)
            .orientation(Orientation::Horizontal)
            .spacing(6.0)
            .vertical_alignment(VerticalAlignment::Center)
            .keyed_children(
                pills
                    .into_iter()
                    .enumerate()
                    .map(|(index, pill)| KeyedView::new(index, pill)),
            );

        Some(
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .column_spacing(10.0)
                .children((
                    summary,
                    Button::new()
                        .grid_column(1)
                        .style(ButtonStyle::Subtle)
                        .is_enabled(idle)
                        .on_click(context.message(Message::MediaRenamer(Action::ClearHistory)))
                        .content(icon_label(GLYPH_DELETE, r.clear_history)),
                )),
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
            return vec![empty_card(GLYPH_HISTORY, r.history_empty)];
        }
        batches
            .iter()
            .map(|batch| batch_card(context, r, batch, idle))
            .collect()
    }

    fn media_renamer_settings_body(&self, context: &ViewContext<Self>) -> Vec<View> {
        let s = &self.snapshot.media_renamer;
        let r = self.t().rename;
        let message = |action: Action| context.message(Message::MediaRenamer(action));
        let dirty = s.template_error.is_none() && s.template != s.default_template;

        let default_template_card = settings_ui::surface(settings_ui::stack(
            vec![
                TextBlock::new()
                    .text(r.default_template)
                    .font_size(13.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .text_wrapping(TextWrapping::Wrap)
                    .into(),
                inset_box(
                    TextBlock::new()
                        .text(s.default_template.as_str())
                        .font_size(12.5)
                        .opacity(0.88)
                        .is_text_selection_enabled(true)
                        .text_wrapping(TextWrapping::Wrap)
                        .into(),
                ),
                hairline(),
                Button::new()
                    .style(if dirty {
                        ButtonStyle::Accent
                    } else {
                        ButtonStyle::Default
                    })
                    .is_enabled(dirty)
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .on_click(message(Action::SaveDefaultTemplate))
                    .content(icon_label(GLYPH_SAVE, r.save_default)),
            ],
            10.0,
        ));

        let recurse_toggle = settings_ui::toggle_switch(
            ToggleSwitch::new()
                .is_on(s.default_recurse)
                .min_width(0.0)
                .on_toggled(
                    context.callback(|on| Message::MediaRenamer(Action::SetDefaultRecurse(on))),
                ),
        );
        let mtime_toggle = settings_ui::toggle_switch(
            ToggleSwitch::new()
                .is_on(s.default_mtime_fallback)
                .min_width(0.0)
                .on_toggled(
                    context
                        .callback(|on| Message::MediaRenamer(Action::SetDefaultMtimeFallback(on))),
                ),
        );
        let lowercase_toggle = settings_ui::toggle_switch(
            ToggleSwitch::new()
                .is_on(s.lowercase_ext)
                .min_width(0.0)
                .on_toggled(
                    context.callback(|on| Message::MediaRenamer(Action::SetLowercaseExt(on))),
                ),
        );

        vec![
            drawer_section(GLYPH_RENAME, r.defaults_section, None, true),
            default_template_card,
            grouped_surface(vec![
                grouped_setting_row(r.default_recurse, None, recurse_toggle),
                grouped_setting_row(r.default_mtime_fallback, None, mtime_toggle),
                grouped_setting_row(
                    r.lowercase_ext,
                    Some(r.lowercase_ext_hint),
                    lowercase_toggle,
                ),
            ]),
            drawer_section(GLYPH_FOLDER, r.explorer_section, None, false),
            inset_notice(GLYPH_INFO, r.explorer_hint),
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
    let selectable_value = |text: String| -> View {
        TextBlock::new()
            .text(text)
            .font_size(12.5)
            .is_text_selection_enabled(true)
            .text_wrapping(TextWrapping::Wrap)
            .into()
    };

    let mut pills = vec![state_pill_view(r, details.state, true)];
    if details.collided {
        pills.push(status_pill(None, r.collided, PillTone::Warning));
    }
    let pill_row = StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(6.0)
        .vertical_alignment(VerticalAlignment::Center)
        .keyed_children(
            pills
                .into_iter()
                .enumerate()
                .map(|(index, pill)| KeyedView::new(index, pill)),
        );

    let camera = [details.make.as_str(), details.model.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    let mut facts = vec![fact_row(
        r.capture_time,
        selectable_value(dash(&details.time)),
    )];
    if let Some(offset) = &details.offset {
        facts.push(fact_row(r.offset, selectable_value(offset.clone())));
    }
    facts.push(fact_row(
        r.source,
        selectable_value(
            details
                .source
                .map(|source| r.source(source))
                .unwrap_or("—")
                .to_string(),
        ),
    ));
    if !camera.is_empty() {
        facts.push(fact_row(r.camera, selectable_value(camera)));
    }

    let summary_card = settings_ui::surface(settings_ui::stack(
        vec![pill_row, hairline(), settings_ui::stack(facts, 7.0)],
        10.0,
    ));

    let member_rows: Vec<View> = details
        .members
        .iter()
        .map(|member| member_row(r, member))
        .collect();

    let mut rows = vec![
        summary_card,
        drawer_section(GLYPH_FOLDER, r.files, Some(details.members.len()), false),
        grouped_surface(member_rows),
    ];

    let multi_media = details.metadata.len() > 1;
    for file in &details.metadata {
        let heading = if multi_media {
            format!("{} · {}", r.metadata, file.name)
        } else {
            r.metadata.to_string()
        };
        let count = match &file.metadata {
            MetadataView::Tags(tags) if !tags.is_empty() => Some(tags.len()),
            _ => None,
        };
        rows.push(drawer_section(GLYPH_INFO, &heading, count, false));
        match &file.metadata {
            MetadataView::Reading => rows.push(settings_ui::surface(
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(10.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        ProgressRing::new()
                            .width(16.0)
                            .height(16.0)
                            .is_active(true)
                            .is_indeterminate(true),
                        settings_ui::caption(r.metadata_reading),
                    )),
            )),
            MetadataView::Tags(tags) if tags.is_empty() => {
                rows.push(compact_empty_row(r.metadata_none));
            }
            MetadataView::Tags(tags) => rows.extend(tag_cards(r, tags)),
        }
    }
    rows
}

fn member_row(r: &RenameStrings, member: &MemberView) -> View {
    let mut badges: Vec<View> = Vec::new();
    if member.primary {
        badges.push(status_pill(None, r.primary, PillTone::Neutral));
    } else if member.companion {
        badges.push(status_pill(None, r.companion, PillTone::Muted));
    }
    match member.probe {
        ProbeView::Pending => {
            badges.push(status_pill(None, r.state_pending, PillTone::Neutral));
        }
        ProbeView::Read => {}
        ProbeView::OnlineOnly => {
            badges.push(status_pill(None, r.state_online_only, PillTone::Warning));
        }
        ProbeView::Unreadable => {
            badges.push(status_pill(
                Some(GLYPH_ERROR),
                r.state_unreadable,
                PillTone::Critical,
            ));
        }
        ProbeView::CodecMissing(codec) => {
            badges.push(status_pill(
                Some(GLYPH_ERROR),
                &r.state_codec_missing.replace("{codec}", codec),
                PillTone::Critical,
            ));
        }
    }

    let name_block = TextBlock::new()
        .text(member.name.as_str())
        .font_size(13.0)
        .font_weight(FontWeight::SEMI_BOLD)
        .is_text_selection_enabled(true)
        .text_wrapping(TextWrapping::Wrap)
        .vertical_alignment(VerticalAlignment::Center);

    let top_line: View = if badges.is_empty() {
        name_block.into()
    } else {
        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(8.0)
            .children((
                name_block.grid_column(0),
                StackPanel::new()
                    .grid_column(1)
                    .orientation(Orientation::Horizontal)
                    .spacing(5.0)
                    .vertical_alignment(VerticalAlignment::Top)
                    .keyed_children(
                        badges
                            .into_iter()
                            .enumerate()
                            .map(|(index, badge)| KeyedView::new(index, badge)),
                    ),
            ))
    };

    let mut lines = vec![top_line];
    if !member.new_name.is_empty() && member.new_name != member.name {
        lines.push(
            TextBlock::new()
                .text(format!("→ {}", member.new_name))
                .font_size(12.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .foreground(ThemeBrush::AccentText)
                .is_text_selection_enabled(true)
                .text_wrapping(TextWrapping::Wrap)
                .into(),
        );
    }

    Border::new().padding(Thickness::xy(14.0, 10.0)).content(
        Grid::new()
            .columns([GridLength::Auto, GridLength::STAR])
            .column_spacing(10.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Top)
                    .children((media_kind_tile(member.kind, MEMBER_TILE_SIZE),)),
                StackPanel::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((settings_ui::stack(lines, 3.0),)),
            )),
    )
}

/// One card per metadata section, listing each tag's name beside its value. Tags arrive
/// ordered by section, so a section is a run of equal labels.
fn tag_cards(r: &RenameStrings, tags: &[ExifTag]) -> Vec<View> {
    tags.chunk_by(|a, b| a.section == b.section)
        .map(|run| {
            let label = r.metadata_section(&run[0].section);
            let header = Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .column_spacing(8.0)
                .children((
                    TextBlock::new()
                        .grid_column(0)
                        .text(label)
                        .font_size(13.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .vertical_alignment(VerticalAlignment::Center),
                    StackPanel::new()
                        .grid_column(1)
                        .vertical_alignment(VerticalAlignment::Center)
                        .children((status_pill(None, &run.len().to_string(), PillTone::Neutral),)),
                ));
            let mut lines = vec![header, hairline()];
            lines.extend(run.iter().map(tag_line));
            settings_ui::surface(settings_ui::stack(lines, 7.0))
        })
        .collect()
}

fn tag_line(tag: &ExifTag) -> View {
    Grid::new()
        .columns([GridLength::Pixel(TAG_NAME_WIDTH), GridLength::STAR])
        .column_spacing(12.0)
        .children((
            TextBlock::new()
                .grid_column(0)
                .text(tag.name.as_str())
                .font_size(12.0)
                .text_wrapping(TextWrapping::Wrap)
                .opacity(0.68)
                .is_text_selection_enabled(true),
            TextBlock::new()
                .grid_column(1)
                .text(tag.value.as_str())
                .font_size(12.5)
                .text_wrapping(TextWrapping::Wrap)
                .is_text_selection_enabled(true),
        ))
}

fn batch_card(
    context: &ViewContext<Shell>,
    r: &RenameStrings,
    batch: &BatchView,
    idle: bool,
) -> View {
    let (kind_label, kind_glyph) = match batch.kind {
        BatchKind::Rename => (r.batch_rename, GLYPH_RENAME),
        BatchKind::Undo => (r.batch_undo, GLYPH_UNDO),
    };
    let (state_pill, stripe_brush): (View, Brush) = match batch.state {
        BatchState::Done if batch.failed == 0 => (
            status_pill(Some(GLYPH_CHECK), r.batch_done, PillTone::Success),
            Color::rgb(0x2D, 0x8A, 0x4E).into(),
        ),
        BatchState::Done => (
            status_pill(Some(GLYPH_WARNING), r.batch_done, PillTone::Warning),
            Color::rgb(0x9A, 0x67, 0x00).into(),
        ),
        BatchState::Interrupted => (
            status_pill(Some(GLYPH_WARNING), r.batch_interrupted, PillTone::Warning),
            Color::rgb(0x9A, 0x67, 0x00).into(),
        ),
        BatchState::Undone => (
            status_pill(Some(GLYPH_UNDO), r.batch_undone, PillTone::Muted),
            Color::rgb(0x70, 0x74, 0x7A).into(),
        ),
        BatchState::Pending => (
            status_pill(None, r.batch_pending, PillTone::Neutral),
            ThemeBrush::Accent.into(),
        ),
    };

    let op_badge = Border::new()
        .width(28.0)
        .height(28.0)
        .corner_radius(CornerRadius::uniform(6.0))
        .background(ThemeBrush::SolidBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .vertical_alignment(VerticalAlignment::Center)
        .content(sized_icon(kind_glyph, 13.0));

    let mut badges = vec![state_pill];
    if batch.failed > 0 {
        badges.push(status_pill(
            Some(GLYPH_ERROR),
            &RenameStrings::count(r.batch_failed, batch.failed as usize),
            PillTone::Critical,
        ));
    }

    let meta_text = format!(
        "{}  ·  {}",
        batch.at,
        RenameStrings::count(r.batch_files, batch.files)
    );

    let header = Grid::new()
        .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
        .column_spacing(10.0)
        .children((
            StackPanel::new()
                .grid_column(0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((op_badge,)),
            StackPanel::new()
                .grid_column(1)
                .spacing(1.0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    TextBlock::new()
                        .text(kind_label)
                        .font_size(13.5)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .text_wrapping(TextWrapping::Wrap),
                    TextBlock::new()
                        .text(meta_text)
                        .font_size(11.5)
                        .opacity(0.65),
                )),
            StackPanel::new()
                .grid_column(2)
                .orientation(Orientation::Horizontal)
                .spacing(5.0)
                .vertical_alignment(VerticalAlignment::Top)
                .keyed_children(
                    badges
                        .into_iter()
                        .enumerate()
                        .map(|(index, badge)| KeyedView::new(index, badge)),
                ),
        ));

    let mut parts = vec![header];
    if !batch.sample.is_empty() {
        let sample_lines: Vec<View> = batch
            .sample
            .iter()
            .map(|(from, to)| {
                TextBlock::new()
                    .text(format!("{from}  →  {to}"))
                    .font_size(11.5)
                    .opacity(0.84)
                    .is_text_selection_enabled(true)
                    .text_wrapping(TextWrapping::Wrap)
                    .into()
            })
            .collect();
        parts.push(inset_box(settings_ui::stack(sample_lines, 3.0)));
    }
    if batch.undoable {
        let id = batch.id;
        parts.push(hairline());
        parts.push(
            Button::new()
                .is_enabled(idle)
                .horizontal_alignment(HorizontalAlignment::Right)
                .on_click(context.message(Message::MediaRenamer(Action::Undo(id))))
                .content(icon_label(GLYPH_UNDO, r.undo)),
        );
    }

    Border::new()
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .corner_radius(CornerRadius::uniform(8.0))
        .content(
            Grid::new().children((
                Border::new()
                    .width(3.0)
                    .corner_radius(CornerRadius::uniform(2.0))
                    .background(stripe_brush)
                    .horizontal_alignment(HorizontalAlignment::Left)
                    .vertical_alignment(VerticalAlignment::Stretch)
                    .margin(Thickness::new(4.0, 8.0, 0.0, 8.0)),
                Border::new()
                    .padding(Thickness::new(15.0, 11.0, 14.0, 11.0))
                    .content(settings_ui::stack(parts, 8.0)),
            )),
        )
}

/// Counts `(cleanly completed, interrupted or with failures, undone)` batches for the
/// History sticky summary bar.
fn batch_counts(batches: &[BatchView]) -> (usize, usize, usize) {
    batches
        .iter()
        .fold((0, 0, 0), |(done, warn, undone), batch| match batch.state {
            BatchState::Done if batch.failed == 0 => (done + 1, warn, undone),
            BatchState::Done | BatchState::Interrupted => (done, warn + 1, undone),
            BatchState::Undone => (done, warn, undone + 1),
            BatchState::Pending => (done, warn, undone),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_batch(state: BatchState, failed: u32) -> BatchView {
        BatchView {
            id: 1,
            at: "2026-11-21 02:00".into(),
            kind: BatchKind::Rename,
            state,
            files: 4,
            failed,
            undoable: state == BatchState::Done,
            sample: Vec::new(),
        }
    }

    #[test]
    fn batch_counts_separates_clean_warning_and_undone_batches() {
        let batches = [
            sample_batch(BatchState::Done, 0),
            sample_batch(BatchState::Done, 2),
            sample_batch(BatchState::Interrupted, 0),
            sample_batch(BatchState::Undone, 0),
        ];
        assert_eq!(batch_counts(&batches), (1, 2, 1));
    }
}
