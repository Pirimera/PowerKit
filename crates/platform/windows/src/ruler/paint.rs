//! Overlay painting: composes each monitor's frozen or color-keyed surface with
//! marks, the live crosshair and labels, and pushes it with `UpdateLayeredWindow`.
use super::*;

/// Full repaint of every overlay and the toolbar, for changes that affect all of
/// them (mode, preferences, toolbar position). Pointer and capture updates use
/// `refresh` instead.
pub(super) fn redraw() {
    let count = RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        ruler.session.as_mut().map(|session| {
            session.live = cursor_monitor(session);
            session.monitors.len()
        })
    });
    let Some(count) = count else {
        return;
    };
    for index in 0..count {
        paint_monitor(index);
    }
    let toolbar = session_toolbar();
    if !toolbar.is_null() {
        unsafe {
            InvalidateRect(toolbar, null(), 0);
        }
    }
}
/// Repaints only the overlays a pointer, wheel or capture change can alter: the
/// monitor under the cursor, the one that showed the crosshair last time (to clear
/// it) and `also`, the monitor whose marks changed. Each is painted once.
pub(super) fn refresh(also: Option<usize>) {
    let targets = RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        let session = ruler.session.as_mut()?;
        let current = cursor_monitor(session);
        let previous = std::mem::replace(&mut session.live, current);
        Some([also, current, previous])
    });
    let Some(targets) = targets else {
        return;
    };
    for (position, target) in targets.iter().enumerate() {
        if let Some(index) = *target
            && !targets[..position].contains(target)
        {
            paint_monitor(index);
        }
    }
}
fn paint_monitor(index: usize) {
    let ready = RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        let Some(session) = ruler.session.as_mut() else {
            return false;
        };
        let Some(monitor) = session.monitors.get_mut(index) else {
            return false;
        };
        if monitor.hwnd.is_null() {
            return false;
        }
        if monitor
            .dib
            .as_ref()
            .is_none_or(|dib| dib.width != monitor.width || dib.height != monitor.height)
        {
            if let Some(dib) = monitor.dib.as_mut() {
                dib.release();
            }
            monitor.dib = Dib::new(monitor.width, monitor.height).ok();
        }
        monitor.dib.is_some()
    });
    if !ready {
        return;
    }
    RULER.with(|ruler| {
        let ruler = ruler.borrow();
        let Some(session) = ruler.session.as_ref() else {
            return;
        };
        let Some(monitor) = session.monitors.get(index) else {
            return;
        };
        let Some(dib) = monitor.dib.as_ref() else {
            return;
        };
        compose(session, monitor, dib);
        let origin = POINT {
            x: monitor.origin_x,
            y: monitor.origin_y,
        };
        let size = SIZE {
            cx: monitor.width,
            cy: monitor.height,
        };
        let zero = POINT { x: 0, y: 0 };
        unsafe {
            UpdateLayeredWindow(
                monitor.hwnd,
                null_mut(),
                &origin,
                &size,
                dib.dc,
                &zero,
                COLOR_KEY,
                null(),
                ULW_COLORKEY,
            );
        }
    });
}
fn compose(session: &Session, monitor: &Monitor, dib: &Dib) {
    let count = (monitor.width as usize) * (monitor.height as usize);
    {
        // SAFETY: `paint_monitor` sized `dib` to this monitor, so `bits` addresses
        // `count` pixels owned by `dib`. GdiFlush completes earlier GDI drawing on
        // its DC before the CPU writes, and the slice ends before GDI draws again.
        let target = unsafe {
            GdiFlush();
            std::slice::from_raw_parts_mut(dib.bits, count)
        };
        if session.config.continuous {
            target.fill(COLOR_KEY);
        } else {
            let copied = count.min(monitor.pixels.len());
            target[..copied].copy_from_slice(&monitor.pixels[..copied]);
        }
    }
    let cursor = cursor_on(monitor);
    let over_toolbar = toolbar_contains_cursor();
    if let Some(drag) = monitor.marks.drag {
        draw_bounds(dib, session, monitor, drag.span(), Some((drag.x1, drag.y1)));
    }
    for kept in &monitor.marks.kept {
        match session.mode {
            Mode::Bounds => draw_bounds(dib, session, monitor, kept.span, None),
            Mode::Spacing | Mode::Horizontal | Mode::Vertical => draw_cross(
                dib,
                session,
                monitor,
                kept.span,
                kept.cursor_x,
                kept.cursor_y,
            ),
            Mode::None => {}
        }
    }
    if over_toolbar {
        return;
    }
    let Some((x, y)) = cursor else {
        return;
    };
    if session.mode.edges()
        && let Some(span) = detect_edges(
            &monitor.pixels,
            monitor.width,
            monitor.height,
            x,
            y,
            session.config.per_channel,
            session.tolerance,
        )
    {
        draw_cross(dib, session, monitor, span, x, y);
    }
}
fn cursor_point() -> Option<POINT> {
    let mut point = POINT { x: 0, y: 0 };
    (unsafe { GetCursorPos(&mut point) } != 0).then_some(point)
}
fn local_point(monitor: &Monitor, point: POINT) -> Option<(i32, i32)> {
    let x = point.x - monitor.origin_x;
    let y = point.y - monitor.origin_y;
    (x >= 0 && y >= 0 && x < monitor.width && y < monitor.height).then_some((x, y))
}
/// Cursor position in `monitor`'s pixels, when the cursor is on it.
pub(super) fn cursor_on(monitor: &Monitor) -> Option<(i32, i32)> {
    local_point(monitor, cursor_point()?)
}
/// Index of the monitor under the cursor.
pub(super) fn cursor_monitor(session: &Session) -> Option<usize> {
    let point = cursor_point()?;
    session
        .monitors
        .iter()
        .position(|monitor| local_point(monitor, point).is_some())
}
/// True while the cursor is over the toolbar; the live crosshair hides there.
pub(super) fn toolbar_contains_cursor() -> bool {
    let toolbar = session_toolbar();
    if toolbar.is_null() {
        return false;
    }
    let mut cursor = POINT { x: 0, y: 0 };
    let mut rect = RECT::default();
    unsafe {
        GetCursorPos(&mut cursor) != 0
            && GetWindowRect(toolbar, &mut rect) != 0
            && PtInRect(&rect, cursor) != 0
    }
}
fn draw_bounds(
    dib: &Dib,
    session: &Session,
    monitor: &Monitor,
    span: Span,
    cursor: Option<(i32, i32)>,
) {
    let color = line_color(session.config.color);
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, color);
        let brush = GetStockObject(NULL_BRUSH);
        let old_pen = SelectObject(dib.dc, pen);
        let old_brush = SelectObject(dib.dc, brush);
        Rectangle(dib.dc, span.left, span.top, span.right + 1, span.bottom + 1);
        SelectObject(dib.dc, old_pen);
        SelectObject(dib.dc, old_brush);
        DeleteObject(pen);
    }
    let anchor = cursor.unwrap_or(((span.left + span.right) / 2, (span.top + span.bottom) / 2));
    draw_label(
        dib,
        monitor,
        session,
        span,
        Mode::Bounds,
        anchor.0,
        anchor.1,
    );
}
fn draw_cross(dib: &Dib, session: &Session, monitor: &Monitor, span: Span, x: i32, y: i32) {
    let (horizontal, vertical) = session.mode.axes();
    let color = line_color(session.config.color);
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, color);
        let old = SelectObject(dib.dc, pen);
        if horizontal {
            line(dib.dc, span.left, y, span.right, y);
            if session.config.feet {
                foot(dib.dc, span.left, y, false);
                foot(dib.dc, span.right, y, false);
            }
        }
        if vertical {
            line(dib.dc, x, span.top, x, span.bottom);
            if session.config.feet {
                foot(dib.dc, x, span.top, true);
                foot(dib.dc, x, span.bottom, true);
            }
        }
        SelectObject(dib.dc, old);
        DeleteObject(pen);
    }
    draw_label(dib, monitor, session, span, session.mode, x, y);
}
fn line(dc: HDC, x0: i32, y0: i32, x1: i32, y1: i32) {
    unsafe {
        MoveToEx(dc, x0, y0, null_mut());
        LineTo(dc, x1 + (x1 - x0).signum(), y1 + (y1 - y0).signum());
    }
}
fn foot(dc: HDC, x: i32, y: i32, horizontal: bool) {
    if horizontal {
        line(dc, x - FEET_HALF, y, x + FEET_HALF, y);
    } else {
        line(dc, x, y - FEET_HALF, x, y + FEET_HALF);
    }
}
/// The configured line color, nudged off the color key so it never turns transparent.
fn line_color(color: u32) -> u32 {
    if color == COLOR_KEY { KEY_SAFE } else { color }
}
fn draw_label(
    dib: &Dib,
    monitor: &Monitor,
    session: &Session,
    span: Span,
    mode: Mode,
    x: i32,
    y: i32,
) {
    let text = overlay_label(
        span,
        mode,
        session.config.unit,
        monitor.px_to_mm,
        monitor.dpi,
    );
    let mut paint = wide(&text);
    let dpi = monitor.dpi as i32;
    let font = label_font(dib, dpi);
    if font.is_null() {
        return;
    }
    unsafe {
        let previous = SelectObject(dib.dc, font);
        let mut bounds = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        DrawTextW(
            dib.dc,
            paint.as_mut_ptr(),
            -1,
            &mut bounds,
            DT_CALCRECT | DT_NOPREFIX,
        );
        let pad = (6 * dpi / 96).max(4);
        let width = bounds.right - bounds.left + pad * 2;
        let height = bounds.bottom - bounds.top + pad * 2;
        let mut left = if x < monitor.width / 2 {
            x + 16
        } else {
            x - width - 16
        };
        let mut top = if y < monitor.height / 2 {
            y + 16
        } else {
            y - height - 16
        };
        left = left.clamp(4, (monitor.width - width - 4).max(4));
        top = top.clamp(4, (monitor.height - height - 4).max(4));
        let background = CreateSolidBrush(0x001E_1E1E);
        let rect = RECT {
            left,
            top,
            right: left + width,
            bottom: top + height,
        };
        FillRect(dib.dc, &rect, background);
        DeleteObject(background);
        SetBkMode(dib.dc, TRANSPARENT as i32);
        SetTextColor(dib.dc, 0x00FF_FFFF);
        let mut text_rect = RECT {
            left: left + pad,
            top: top + pad,
            right: left + width - pad,
            bottom: top + height - pad,
        };
        DrawTextW(dib.dc, paint.as_mut_ptr(), -1, &mut text_rect, DT_NOPREFIX);
        SelectObject(dib.dc, previous);
    }
}
/// Label font for `dpi`, created once per surface instead of once per label.
fn label_font(dib: &Dib, dpi: i32) -> HFONT {
    let (cached, cached_dpi) = dib.font.get();
    if !cached.is_null() && cached_dpi == dpi {
        return cached;
    }
    unsafe {
        if !cached.is_null() {
            DeleteObject(cached);
        }
        let font = CreateFontW(
            -(14 * dpi / 96),
            0,
            0,
            0,
            FW_SEMIBOLD as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            wide("Segoe UI").as_ptr(),
        );
        dib.font.set((font, dpi));
        font
    }
}
