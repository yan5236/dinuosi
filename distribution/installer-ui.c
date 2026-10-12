/* 仅为 NSIS 安装进度页提供双向详情按钮；插件在安装器临时目录运行，不进入应用运行库。 */
#define UNICODE
#define _UNICODE
#include <windows.h>
#include <commctrl.h>
#include <nsis/pluginapi.h>

static HINSTANCE module;
static HWND page, button, log_view;
static WCHAR show_text[128], hide_text[128];

/* 处理当前进度页的详情按钮点击，按列表自身可见样式切换；其他消息交回原窗口过程。 */
static LRESULT CALLBACK details_proc(HWND window, UINT message, WPARAM wparam,
                                    LPARAM lparam, UINT_PTR id, DWORD_PTR data) {
    (void)data;
    if (message == WM_COMMAND && LOWORD(wparam) == 1027 &&
        HIWORD(wparam) == BN_CLICKED && (HWND)lparam == button) {
        BOOL expanded = (GetWindowLongPtrW(log_view, GWL_STYLE) & WS_VISIBLE) != 0;
        ShowWindow(log_view, expanded ? SW_HIDE : SW_SHOWNA);
        SetWindowTextW(button, expanded ? show_text : hide_text);
        return 0;
    }
    if (message == WM_NCDESTROY) {
        RemoveWindowSubclass(window, details_proc, id);
        page = button = log_view = NULL;
    }
    return DefSubclassProc(window, message, wparam, lparam);
}

/* NSIS 退出 GUI 时兜底移除当前页的挂接，防止插件卸载后遗留函数指针；其余通知返回零。 */
static UINT_PTR plugin_callback(enum NSPIM message) {
    if (message == NSPIM_GUIUNLOAD && page) {
        RemoveWindowSubclass(page, details_proc, 1027);
        page = button = log_view = NULL;
    }
    return 0;
}

/* 从 NSIS 栈读取真实进度页句柄和双语按钮文本，同线程挂接并为按钮预留独立行。
   返回栈字符串 ok/error；任何控件、线程或回调注册失败均拒绝挂接，不启动额外线程。 */
__declspec(dllexport) void Attach(HWND parent, int string_size, LPTSTR variables,
                                stack_t **stacktop, extra_parameters *extra) {
    (void)parent;
    EXDLL_INIT();
    HWND target = (HWND)popintptr();
    int invalid = popstringn(show_text, 128);
    invalid |= popstringn(hide_text, 128);
    HWND target_button = GetDlgItem(target, 1027), target_log = GetDlgItem(target, 1016);
    if (invalid || !IsWindow(target) || !target_button || !target_log || page ||
        GetWindowThreadProcessId(target, NULL) != GetCurrentThreadId() ||
        extra->RegisterPluginCallback(module, plugin_callback) < 0 ||
        !SetWindowSubclass(target, details_proc, 1027, 0)) {
        pushstring(L"error");
        return;
    }
    page = target;
    button = target_button;
    log_view = target_log;
    RECT list, bounds;
    GetWindowRect(log_view, &list);
    GetWindowRect(button, &bounds);
    MapWindowPoints(NULL, page, (POINT *)&list, 2);
    int button_height = bounds.bottom - bounds.top;
    int gap = button_height / 4;
    MoveWindow(button, list.left, list.top, bounds.right - bounds.left, button_height, TRUE);
    MoveWindow(log_view, list.left, list.top + button_height + gap,
               list.right - list.left, list.bottom - list.top - button_height - gap, TRUE);
    SetWindowTextW(button, show_text);
    ShowWindow(log_view, SW_HIDE);
    ShowWindow(button, SW_SHOWNA);
    pushstring(L"ok");
}

/* 页尾显式移除挂接，允许后续页面或下一次安装重新初始化；不修改原有进度日志。 */
__declspec(dllexport) void Detach(HWND parent, int string_size, LPTSTR variables,
                                stack_t **stacktop, extra_parameters *extra) {
    (void)parent; (void)string_size; (void)variables; (void)stacktop; (void)extra;
    if (page) {
        RemoveWindowSubclass(page, details_proc, 1027);
        page = button = log_view = NULL;
    }
}

/* 记录插件模块句柄供 NSIS 回调注册使用；加载器阶段不执行 UI 操作，始终返回成功。 */
BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID reserved) {
    (void)reserved;
    if (reason == DLL_PROCESS_ATTACH) module = instance;
    return TRUE;
}
