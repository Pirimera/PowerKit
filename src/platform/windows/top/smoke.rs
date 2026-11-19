//! AlwaysTop smoke checks against real fixture windows, kept out of production helpers.
use super::*;
pub(in crate::platform::windows) fn smoke(app: HWND) -> io::Result<()> {
    struct Fixture {
        hwnd: HWND,
        class: Vec<u16>,
        instance: windows_sys::Win32::Foundation::HINSTANCE,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            unsafe {
                if !self.hwnd.is_null() {
                    DestroyWindow(self.hwnd);
                }
                UnregisterClassW(self.class.as_ptr(), self.instance);
            }
        }
    }
    forget();
    if toggle_from(app, null_mut()).is_ok() {
        let _ = release();
        return Err(io::Error::other("Aday yokken AlwaysTop açılmamalı"));
    }
    if session().active() {
        return Err(io::Error::other("Başarısız sabitleme oturumu açık bıraktı"));
    }
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class = super::super::wide(&format!("PowerKit.Smoke.AlwaysTop.{}", std::process::id()));
    let wnd = WNDCLASSW {
        lpfnWndProc: Some(DefWindowProcW),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&wnd) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut fixture = Fixture {
        hwnd: unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                super::super::wide("AlwaysTop fixture").as_ptr(),
                WS_OVERLAPPED | WS_CAPTION,
                -32_000,
                -32_000,
                240,
                80,
                null_mut(),
                null_mut(),
                instance,
                std::ptr::null(),
            )
        },
        class,
        instance,
    };
    if fixture.hwnd.is_null() {
        return Err(io::Error::last_os_error());
    }
    if pin_window(fixture.hwnd, app)? != Pin::on("AlwaysTop fixture") || !is_topmost(fixture.hwnd) {
        return Err(io::Error::other("AlwaysTop sabitleme uygulanmadı"));
    }
    if !step_window_opacity(fixture.hwnd, -OPACITY_STEP) || !is_layered(fixture.hwnd) {
        return Err(io::Error::other("AlwaysTop saydamlık uygulanmadı"));
    }
    let mut alpha = 0u8;
    if unsafe { GetLayeredWindowAttributes(fixture.hwnd, null_mut(), &mut alpha, null_mut()) } == 0
        || alpha != alpha_from_percent(90)
    {
        return Err(io::Error::other("AlwaysTop saydamlık uygulanmadı"));
    }
    unpin_window(fixture.hwnd)?;
    if session().active() || is_topmost(fixture.hwnd) || is_layered(fixture.hwnd) {
        return Err(io::Error::other("AlwaysTop sabitleme geri alınmadı"));
    }
    let second = unsafe {
        CreateWindowExW(
            0,
            fixture.class.as_ptr(),
            super::super::wide("AlwaysTop second").as_ptr(),
            WS_OVERLAPPED | WS_CAPTION,
            -32_000,
            -32_000,
            240,
            80,
            null_mut(),
            null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if second.is_null() {
        return Err(io::Error::last_os_error());
    }
    pin_window(fixture.hwnd, app)?;
    pin_window(second, app)?;
    if !is_topmost(fixture.hwnd) || !is_topmost(second) || session() != (Pin::Several { count: 2 })
    {
        unsafe {
            DestroyWindow(second);
        }
        return Err(io::Error::other("AlwaysTop birden fazla pencereyi tutmadı"));
    }
    unpin_window(fixture.hwnd)?;
    if session().title() != Some("AlwaysTop second")
        || is_topmost(fixture.hwnd)
        || !is_topmost(second)
    {
        unsafe {
            DestroyWindow(second);
        }
        return Err(io::Error::other("AlwaysTop diğer pinleri düşürdü"));
    }
    unsafe {
        DestroyWindow(second);
    }
    if !reconcile() || session().active() {
        return Err(io::Error::other(
            "Kapanan pencere AlwaysTop oturumunu kapatmadı",
        ));
    }
    pin_window(fixture.hwnd, app)?;
    unsafe {
        DestroyWindow(fixture.hwnd);
    }
    fixture.hwnd = null_mut();
    if !reconcile() || session().active() {
        return Err(io::Error::other(
            "Kapanan pencere AlwaysTop oturumunu kapatmadı",
        ));
    }
    ring_lifecycle(app, &fixture.class, instance)
}
/// Ring follows hide, show and cloak; a child's destroy keeps the pin; a destroyed
/// target is unpinned by the ring timer path. Uses a visible tool window far off
/// screen, so nothing appears on the desktop or taskbar.
fn ring_lifecycle(
    app: HWND,
    class: &[u16],
    instance: windows_sys::Win32::Foundation::HINSTANCE,
) -> io::Result<()> {
    struct Window(HWND);
    impl Drop for Window {
        fn drop(&mut self) {
            if live(self.0) {
                let _ = unpin_window(self.0);
                unsafe {
                    DestroyWindow(self.0);
                }
            }
        }
    }
    fn fail(message: &str) -> io::Result<()> {
        Err(io::Error::other(message.to_owned()))
    }
    fn ring_visible(target: HWND) -> bool {
        let border = border_of(target);
        live(border) && unsafe { IsWindowVisible(border) } != 0
    }
    fn set_cloak(hwnd: HWND, cloak: bool) -> bool {
        let value = i32::from(cloak);
        // SAFETY: hwnd is our own window; value is a live BOOL for the call.
        unsafe {
            DwmSetWindowAttribute(
                hwnd,
                windows_sys::Win32::Graphics::Dwm::DWMWA_CLOAK as u32,
                (&raw const value).cast(),
                4,
            ) == 0
        }
    }
    // SAFETY: class is registered by the caller and outlives this window.
    let target = Window(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            super::super::wide("AlwaysTop ring").as_ptr(),
            WS_POPUP | WS_VISIBLE,
            -32_000,
            -32_000,
            240,
            80,
            null_mut(),
            null_mut(),
            instance,
            std::ptr::null(),
        )
    });
    let hwnd = target.0;
    if hwnd.is_null() {
        return Err(io::Error::last_os_error());
    }
    pin_window(hwnd, app)?;
    let border = border_of(hwnd);
    if !ring_visible(hwnd) {
        return fail("AlwaysTop çerçevesi gösterilmedi");
    }
    unsafe {
        ShowWindow(hwnd, SW_HIDE);
    }
    on_event(EVENT_OBJECT_HIDE, hwnd, app);
    if ring_visible(hwnd) {
        return fail("Gizlenen pencerenin AlwaysTop çerçevesi kaldı");
    }
    unsafe {
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    on_event(EVENT_OBJECT_SHOW, hwnd, app);
    if !ring_visible(hwnd) {
        return fail("Yeniden gösterilen pencerenin AlwaysTop çerçevesi dönmedi");
    }
    if border_of(hwnd) != border {
        return fail("Gizleme ve gösterme AlwaysTop çerçevesini yeniden oluşturdu");
    }
    on_event(EVENT_SYSTEM_MINIMIZESTART, hwnd, app);
    if ring_visible(hwnd) || border_of(hwnd) != border {
        return fail("Simge durumuna küçültülen pencerenin AlwaysTop çerçevesi kaldı");
    }
    on_event(EVENT_SYSTEM_MINIMIZEEND, hwnd, app);
    if !ring_visible(hwnd) || border_of(hwnd) != border {
        return fail("Geri yüklenen pencerenin AlwaysTop çerçevesi dönmedi");
    }
    if set_cloak(hwnd, true) {
        on_event(EVENT_OBJECT_CLOAKED, hwnd, app);
        if ring_visible(hwnd) {
            return fail("Gizlenmiş (cloaked) pencerenin AlwaysTop çerçevesi kaldı");
        }
        set_cloak(hwnd, false);
        on_event(EVENT_OBJECT_UNCLOAKED, hwnd, app);
        if !ring_visible(hwnd) {
            return fail("Cloak kalkınca AlwaysTop çerçevesi dönmedi");
        }
    }
    let static_class = super::super::wide("STATIC");
    // SAFETY: a child of our own fixture, destroyed below on this thread.
    let child = unsafe {
        CreateWindowExW(
            0,
            static_class.as_ptr(),
            std::ptr::null(),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            10,
            10,
            hwnd,
            null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if child.is_null() {
        return Err(io::Error::last_os_error());
    }
    on_event(EVENT_OBJECT_DESTROY, child, app);
    unsafe {
        DestroyWindow(child);
    }
    if !tracked(hwnd) {
        return fail("Alt pencerenin kapanması AlwaysTop sabitlemesini kaldırdı");
    }
    unsafe {
        DestroyWindow(hwnd);
    }
    // The ring's backup timer calls sync_border with the now-dead target.
    sync_border(hwnd);
    if tracked(hwnd) || live(border) || session().active() {
        return fail("Kapanan pencerenin AlwaysTop çerçevesi kaldı");
    }
    Ok(())
}
