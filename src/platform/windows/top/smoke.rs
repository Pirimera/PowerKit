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
    if toggle(app).is_ok() {
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
    if !is_topmost(fixture.hwnd) || !is_topmost(second) || session() != Pin::on("2 pencere") {
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
    Ok(())
}
