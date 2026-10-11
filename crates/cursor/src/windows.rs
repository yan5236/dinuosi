use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{cell::Cell, rc::Rc};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
    Graphics::Gdi::ScreenToClient,
    System::Threading::GetCurrentThreadId,
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            CreateIconFromResourceEx, DestroyCursor, GetCursor, GetCursorPos,
            GetWindowThreadProcessId, HCURSOR, HTCLIENT, IDC_ARROW, LR_DEFAULTCOLOR, LoadCursorW,
            SetCursor, WM_MOUSEMOVE, WM_NCDESTROY, WM_SETCURSOR, WindowFromPoint,
        },
    },
};

struct State {
    window: HWND,
    cursor: HCURSOR,
    region: Cell<Option<[i32; 4]>>,
    attached: Cell<bool>,
}

/// 只操作创建时校验的本线程窗口；Rc 使对象不能发送至其他线程。
pub struct NativeCursor {
    state: Rc<State>,
    callback_data: *mut Rc<State>,
}

impl NativeCursor {
    /// 为窗口安装原生光标，像素来自既有 SVG；无窗口句柄、跨线程或 Win32 失败返回错误。
    /// 注册数据由子类持有至销毁或显式移除；创建失败释放已创建的系统资源。
    pub fn new(
        window: &impl HasWindowHandle,
        rgba: &[u8],
        width: u32,
        height: u32,
        hotspot: (u16, u16),
    ) -> Result<Self, String> {
        let RawWindowHandle::Win32(handle) =
            window.window_handle().map_err(|e| e.to_string())?.as_raw()
        else {
            return Err("窗口没有 Windows 原生句柄".into());
        };
        let window = HWND(handle.hwnd.get() as *mut _);
        // SAFETY: 句柄来自借用的窗口；检查归属线程后才安装子类，后续接口均由不可 Send 的对象调用。
        if unsafe { GetWindowThreadProcessId(window, None) != GetCurrentThreadId() } {
            return Err("原生光标必须在窗口所属线程创建".into());
        }
        let bytes = crate::cursor_resource(rgba, width, height, hotspot)?;
        // SAFETY: 已验证尺寸和完整 RT_CURSOR 缓冲；系统复制数据，不保留 Vec 地址。
        let icon = unsafe {
            CreateIconFromResourceEx(
                &bytes,
                false,
                0x30000,
                width as i32,
                height as i32,
                LR_DEFAULTCOLOR,
            )
        }
        .map_err(|e| format!("创建原生光标失败：{e}"))?;
        let state = Rc::new(State {
            window,
            cursor: HCURSOR(icon.0),
            region: Cell::new(None),
            attached: Cell::new(false),
        });
        let callback_data = Box::into_raw(Box::new(state.clone()));
        // SAFETY: Box 的稳定地址由回调持有，唯一 ID 使用该地址；移除或 WM_NCDESTROY 后仅释放一次。
        if !unsafe {
            SetWindowSubclass(
                window,
                Some(cursor_proc),
                callback_data as usize,
                callback_data as usize,
            )
        }
        .as_bool()
        {
            // SAFETY: 注册失败，Windows 没有持有此指针。
            unsafe {
                drop(Box::from_raw(callback_data));
            }
            return Err("安装原生光标窗口回调失败".into());
        }
        state.attached.set(true);
        Ok(Self {
            state,
            callback_data,
        })
    }

    /// 更新以设备像素表示的半开活动矩形；None 恢复系统指针，菜单和其他手势由调用方关闭区域。
    /// 同步应用当前指针，移动本身由系统光标完成，不等待下一次 GPUI 绘制。
    pub fn set_region(&self, region: Option<[i32; 4]>) {
        self.state.region.set(region);
        self.state.apply();
    }
}

impl State {
    /// 根据实时系统指针与窗口遮挡判断活动区域；窗口外、其他窗口和非活动位置不覆盖系统指针。
    /// 只在当前指针确由本对象设置时恢复箭头，避免影响文本框和缩放把手的系统光标。
    fn apply(&self) {
        if !self.attached.get() {
            return;
        }
        // SAFETY: 同线程、仍注册的窗口；POINT 是完整可写结构，系统函数不保存其地址。
        unsafe {
            let mut position = POINT::default();
            let inside = GetCursorPos(&mut position).is_ok()
                && WindowFromPoint(position) == self.window
                && ScreenToClient(self.window, &mut position).as_bool()
                && self.region.get().is_some_and(|[left, top, right, bottom]| {
                    position.x >= left
                        && position.x < right
                        && position.y >= top
                        && position.y < bottom
                });
            if inside {
                SetCursor(Some(self.cursor));
            } else if GetCursor() == self.cursor
                && let Ok(arrow) = LoadCursorW(None, IDC_ARROW)
            {
                SetCursor(Some(arrow));
            }
        }
    }
}

/// 保留 GPUI 的全部窗口消息；仅在客户区光标消息及鼠标移动处理后替换平台指针。
/// 回调先克隆 Rc，允许下游销毁窗口；WM_NCDESTROY 移除注册并释放 Windows 持有的 Box。
unsafe extern "system" fn cursor_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: data 是成功注册后尚未移除的 Box<Rc<State>>，由本函数或 Drop 独占释放。
    let state = unsafe { (&*(data as *const Rc<State>)).clone() };
    if message == WM_NCDESTROY {
        state.region.set(None);
        state.apply();
        state.attached.set(false);
        // SAFETY: 销毁消息只执行一次，移除后系统不会再次使用回调数据。
        unsafe {
            let _ = RemoveWindowSubclass(window, Some(cursor_proc), id);
            drop(Box::from_raw(data as *mut Rc<State>));
        }
    }
    // SAFETY: 原样转发系统提供的消息参数，不截获窗口移动、尺寸或编辑操作。
    let result = unsafe { DefSubclassProc(window, message, wparam, lparam) };
    if message == WM_MOUSEMOVE || (message == WM_SETCURSOR && lparam.0 as u16 as u32 == HTCLIENT) {
        state.apply();
    }
    result
}

impl Drop for NativeCursor {
    /// 先恢复光标再移除窗口回调；窗口先销毁时由 WM_NCDESTROY 清理，避免重复释放。
    fn drop(&mut self) {
        self.set_region(None);
        if self.state.attached.get() {
            // SAFETY: 同线程且注册仍存活，成功移除后才释放回调 Box；失败保留数据避免悬垂指针。
            if unsafe {
                RemoveWindowSubclass(
                    self.state.window,
                    Some(cursor_proc),
                    self.callback_data as usize,
                )
            }
            .as_bool()
            {
                self.state.attached.set(false);
                unsafe {
                    drop(Box::from_raw(self.callback_data));
                }
            }
        }
    }
}

impl Drop for State {
    /// 所有窗口回调和 GUI 引用消失后释放私有系统光标，不销毁共享系统箭头。
    fn drop(&mut self) {
        // SAFETY: cursor 是本对象由 CreateIconFromResourceEx 创建且仅在此释放的私有句柄。
        if let Err(error) = unsafe { DestroyCursor(self.cursor) } {
            eprintln!("释放原生光标失败：{error}");
        }
    }
}
