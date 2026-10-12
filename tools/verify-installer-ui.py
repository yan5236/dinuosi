"""真实验证安装详情折叠、完成页选项与卸载确认页；只使用独占测试安装和隔离用户数据。"""
import ctypes as c
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import time
import uuid
import winreg

ROOT = Path(__file__).resolve().parent.parent
QA = ROOT / '.local/installer-flow' / uuid.uuid4().hex
USER = c.WinDLL('user32', use_last_error=True)
KERNEL = c.WinDLL('kernel32', use_last_error=True)
SHELL = c.WinDLL('shell32', use_last_error=True)
ENUM = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
USER.EnumWindows.argtypes = [ENUM, w.LPARAM]
USER.EnumChildWindows.argtypes = [w.HWND, ENUM, w.LPARAM]
USER.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
USER.GetWindowTextW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
USER.GetDlgCtrlID.argtypes = [w.HWND]
USER.GetParent.argtypes = [w.HWND]
USER.GetParent.restype = w.HWND
USER.GetWindowLongPtrW.argtypes = [w.HWND, c.c_int]
USER.GetWindowLongPtrW.restype = c.c_ssize_t
USER.IsWindowEnabled.argtypes = [w.HWND]
USER.IsWindowVisible.argtypes = [w.HWND]
USER.SendMessageTimeoutW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM, w.UINT, w.UINT, c.POINTER(c.c_size_t)]
USER.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
KERNEL.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
KERNEL.OpenProcess.restype = w.HANDLE
KERNEL.QueryFullProcessImageNameW.argtypes = [w.HANDLE, w.DWORD, w.LPWSTR, c.POINTER(w.DWORD)]
KERNEL.CloseHandle.argtypes = [w.HANDLE]
KERNEL.WaitForSingleObject.argtypes = [w.HANDLE, w.DWORD]
KERNEL.WaitForSingleObject.restype = w.DWORD
KERNEL.CreateFileW.argtypes = [w.LPCWSTR, w.DWORD, w.DWORD, c.c_void_p, w.DWORD, w.DWORD, w.HANDLE]
KERNEL.CreateFileW.restype = w.HANDLE
SHELL.SHGetFolderPathW.argtypes = [w.HWND, c.c_int, w.HANDLE, w.DWORD, w.LPWSTR]
KEY = 'Software\\Dinuosi\\Installer'


def folder(identifier):
    """读取当前用户实际桌面或开始菜单目录，支持重定向；系统查询失败时拒绝执行验收。"""
    text = c.create_unicode_buffer(260)
    assert SHELL.SHGetFolderPathW(None, identifier, None, 0, text) == 0
    return Path(text.value)


def registered():
    """判断是否已有 Dinuosi 安装登记；只有键确实不存在才返回 False，权限错误向上抛出。"""
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, KEY):
            return True
    except FileNotFoundError:
        return False


def text(window):
    """以限时 WM_GETTEXT 读取真实控件内容，兼容跨进程编辑框；窗口消失或无响应时返回空串。"""
    value = c.create_unicode_buffer(2048)
    result = c.c_size_t()
    if not USER.SendMessageTimeoutW(window, 0x000D, len(value), c.cast(value, c.c_void_p).value, 2, 1000, c.byref(result)):
        return ''
    return value.value


def windows(pid=None):
    """枚举当前桌面的真实顶层窗口，可按本轮进程 PID 过滤；返回句柄与 PID，不猜测窗口标识。"""
    result = []

    @ENUM
    def collect(window, extra):
        """读取窗口所属 PID 并继续枚举，只保留调用方指定进程或全部窗口的记录。"""
        owner = w.DWORD()
        USER.GetWindowThreadProcessId(window, c.byref(owner))
        if pid is None or owner.value == pid:
            result.append((window, owner.value))
        return True

    USER.EnumWindows(collect, 0)
    return result


def controls(window):
    """枚举验收主窗口下的实际控件；返回句柄列表，包含暂时隐藏的目录和详情控件。"""
    result = []

    @ENUM
    def collect(child, extra):
        """记录真实子窗口句柄并继续枚举，不改写控件或页面。"""
        result.append(child)
        return True

    USER.EnumChildWindows(window, collect, 0)
    return result


def visible(control, main):
    """检查控件到验收主窗口之间的可见样式；忽略测试主窗口的隐藏启动状态，以核对页面实际布局。"""
    while control and control != main:
        if not USER.GetWindowLongPtrW(control, -16) & 0x10000000:
            return False
        control = USER.GetParent(control)
    return control == main


def control_id(main, identifier, shown=True):
    """从真实控件树按 ID 查找当前页控件，默认排除旧页或隐藏控件；找不到返回 None。"""
    return next((child for child in controls(main) if USER.GetDlgCtrlID(child) == identifier
                 and (not shown or visible(child, main))), None)


def wait(description, predicate, timeout=60):
    """轮询实际窗口或文件状态直到满足断言条件；超时抛错并保留现场，不把未出现的状态计作成功。"""
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        result = predicate()
        if result:
            return result
        time.sleep(0.03)
    raise AssertionError('等待超时：' + description)


def message(window, kind, first=0, second=0):
    """向刚定位到的验收控件发送标准原生测试消息；等待最多十秒，失败或挂起直接断言失败。"""
    result = c.c_size_t()
    assert USER.SendMessageTimeoutW(window, kind, first, second, 2, 10000, c.byref(result))
    return result.value


def click(window):
    """异步触发控件原生事件；隐藏窗口的导航按钮使用等价 WM_COMMAND，其余控件走 BM_CLICK 自动切换。

    不直接调用产品脚本回调；调用方重新等待实际页面状态，避免自定义页消息循环阻塞发送方。
    """
    if USER.GetDlgCtrlID(window) == 1:
        assert USER.PostMessageW(USER.GetParent(window), 0x0111, 1, window)
    else:
        assert USER.PostMessageW(window, 0x00F5, 0, 0)


def application_windows(installation):
    """按程序完整路径查找本轮安装的 Dinuosi 窗口，不把其他用户打开的同名程序算入验收。"""
    expected = str(installation / 'dinuosi.exe').casefold()
    result = []
    for window, pid in windows():
        process = KERNEL.OpenProcess(0x1000, False, pid)
        if not process:
            continue
        try:
            path, length = c.create_unicode_buffer(32768), w.DWORD(32768)
            if (KERNEL.QueryFullProcessImageNameW(process, 0, path, c.byref(length)) and path.value.casefold() == expected
                    and text(window) == 'Dinuosi' and USER.IsWindowVisible(window)):
                result.append(window)
        finally:
            KERNEL.CloseHandle(process)
    return result


def executable_released(installation):
    """按安装器同一共享规则尝试打开本轮 EXE，不写字节；确认进程退出后系统的暂时映像占用也已释放。"""
    handle = KERNEL.CreateFileW(str(installation / 'dinuosi.exe'), 0x40000000, 3, None, 3, 0, None)
    if handle in (None, c.c_void_p(-1).value):
        return False
    KERNEL.CloseHandle(handle)
    return True


def snapshot(window, path):
    """用 PrintWindow 保存本轮验收窗口，支持隐藏或遮挡窗口；只读绘制结果并释放本次 GDI 对象。"""
    from PIL import Image
    gdi = c.WinDLL('gdi32', use_last_error=True)
    USER.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
    USER.GetWindowDC.argtypes = [w.HWND]
    USER.GetWindowDC.restype = w.HDC
    USER.ReleaseDC.argtypes = [w.HWND, w.HDC]
    USER.PrintWindow.argtypes = [w.HWND, w.HDC, w.UINT]
    gdi.CreateCompatibleDC.argtypes = [w.HDC]
    gdi.CreateCompatibleDC.restype = w.HDC
    gdi.CreateCompatibleBitmap.argtypes = [w.HDC, c.c_int, c.c_int]
    gdi.CreateCompatibleBitmap.restype = w.HBITMAP
    gdi.SelectObject.argtypes = [w.HDC, w.HGDIOBJ]
    gdi.SelectObject.restype = w.HGDIOBJ
    gdi.GetDIBits.argtypes = [w.HDC, w.HBITMAP, w.UINT, w.UINT, c.c_void_p, c.c_void_p, w.UINT]
    gdi.DeleteObject.argtypes = [w.HGDIOBJ]
    gdi.DeleteDC.argtypes = [w.HDC]
    rect = w.RECT()
    assert USER.GetWindowRect(window, c.byref(rect))
    width, height = rect.right - rect.left, rect.bottom - rect.top
    dc = USER.GetWindowDC(window)
    memory = gdi.CreateCompatibleDC(dc)
    bitmap = gdi.CreateCompatibleBitmap(dc, width, height)
    previous = gdi.SelectObject(memory, bitmap)
    try:
        assert USER.PrintWindow(window, memory, 2)
        gdi.SelectObject(memory, previous)
        header = c.create_string_buffer(struct.pack('<IiiHHIIiiII', 40, width, -height, 1, 32, 0, width * height * 4, 0, 0, 0, 0))
        pixels = c.create_string_buffer(width * height * 4)
        assert gdi.GetDIBits(memory, bitmap, 0, height, pixels, header, 0)
        Image.frombytes('RGB', (width, height), pixels.raw, 'raw', 'BGRX').save(path)
    finally:
        gdi.DeleteObject(bitmap)
        gdi.DeleteDC(memory)
        USER.ReleaseDC(window, dc)


def verify_case(language, desktop, run):
    """验证一种语言与勾选组合的完整交互安装，确认副作用仅发生在完成点击后，再正常关闭和卸载。

    拒绝覆盖正式安装或快捷方式；安装与数据位于 QA，临时写当前用户登记及快捷方式，完成后清理。
    """
    assert not registered() and not DESKTOP.exists() and not START_MENU.exists(), '存在已有安装或快捷方式，拒绝验收'
    case = QA / f'{language}-{int(desktop)}-{int(run)}'
    installation, data, temporary = case / '安装 中文', case / '用户数据', case / 'temp'
    assert installation.resolve().is_relative_to(QA.resolve())
    data.mkdir(parents=True)
    temporary.mkdir()
    proof = data / '保留.txt'
    proof.write_text('用户数据保留', encoding='utf-8')
    env = dict(os.environ, DINUOSI_DATA_DIR=str(data), TEMP=str(temporary), TMP=str(temporary))
    with winreg.CreateKey(winreg.HKEY_CURRENT_USER, KEY) as key:
        winreg.SetValueEx(key, 'Language', 0, winreg.REG_SZ, '2052' if language == 'zh-CN' else '1033')
    startup = subprocess.STARTUPINFO()
    startup.dwFlags = subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    # NSIS 的末尾 /D 参数要求保留原始命令行；通用参数数组会把整个带空格参数加引号而被忽略。
    process = subprocess.Popen(f'"{SETUP}" /D={installation}', env=env, startupinfo=startup)
    (case / 'process.json').write_text(json.dumps({'pid': process.pid, 'installation': str(installation)}), encoding='utf-8')
    dialog = wait('语言选择窗口', lambda: next((handle for handle, _ in windows(process.pid) if text(handle) == 'Dinuosi - Language / 语言'), None))
    click(control_id(dialog, 1))
    main = wait('安装目录页准备完成', lambda: next((handle for handle, _ in windows(process.pid)
        if text(handle).startswith('Dinuosi') and text(handle) != 'Dinuosi - Language / 语言'
        and any(text(child) == str(installation) and visible(child, handle) for child in controls(handle))
        and control_id(handle, 1) and USER.IsWindowEnabled(control_id(handle, 1))), None))
    click(control_id(main, 1))
    button = wait('可折叠详情按钮', lambda: control_id(main, 1027))
    log = control_id(main, 1016, shown=False)
    assert log and not visible(log, main)
    if desktop and run:
        snapshot(main, case / 'details-collapsed.png')
    for expanded in (True, False, True, False):
        click(button)
        expected = ({True: '收起详情', False: '显示详情'} if language == 'zh-CN' else {True: 'Hide details', False: 'Show details'})[expanded]
        actual = text(button)
        try:
            wait('详情按钮状态与文字同步', lambda: visible(log, main) == expanded and text(button) == expected, timeout=2)
        except AssertionError:
            print(f'details expected={expected!r} actual={actual!r} visible={visible(log, main)}', flush=True)
            snapshot(main, case / 'details-failed.png')
            raise
        if expanded and desktop and run:
            snapshot(main, case / 'details-expanded.png')
    try:
        wait('安装结束后的下一步', lambda: control_id(main, 1) and USER.IsWindowEnabled(control_id(main, 1)))
    except AssertionError:
        print('installer exit code', process.poll(), flush=True)
        for handle, _ in windows(process.pid):
            print('installer window', repr(text(handle)), [(USER.GetDlgCtrlID(child), text(child), visible(child, handle)) for child in controls(handle)], flush=True)
        snapshot(main, case / 'installation-timeout.png')
        raise
    assert process.poll() is None, '安装器在安装结束后自行关闭'
    assert message(log, 0x1004) > 0, '安装详情为空'
    assert not DESKTOP.exists() and not application_windows(installation), '点击完成前触发了副作用'
    click(control_id(main, 1))
    shortcut_text = '创建桌面快捷方式' if language == 'zh-CN' else 'Create a desktop shortcut'
    run_text = '立即运行 Dinuosi' if language == 'zh-CN' else 'Run Dinuosi now'
    shortcut = wait('创建快捷方式勾选项', lambda: next((child for child in controls(main) if text(child) == shortcut_text and visible(child, main)), None))
    launch = wait('立即运行勾选项', lambda: next((child for child in controls(main) if text(child) == run_text and visible(child, main)), None))
    assert text(control_id(main, 1)) == ('完成' if language == 'zh-CN' else 'Finish')
    assert message(shortcut, 0x00F0) == message(launch, 0x00F0) == 1
    assert not DESKTOP.exists() and not application_windows(installation), '仅展示完成页就执行了勾选项'
    if not desktop:
        click(shortcut)
    if not run:
        click(launch)
    if desktop and run:
        snapshot(main, case / 'finish.png')
    click(control_id(main, 1))
    assert process.wait(timeout=15) == 0
    assert DESKTOP.exists() == desktop
    if run:
        app_windows = wait('完成后启动的软件窗口', lambda: application_windows(installation), timeout=15)
        owner = w.DWORD()
        USER.GetWindowThreadProcessId(app_windows[0], c.byref(owner))
        application = KERNEL.OpenProcess(0x100000, False, owner.value)
        assert application
        try:
            for window in app_windows:
                USER.PostMessageW(window, 0x0010, 0, 0)
            assert KERNEL.WaitForSingleObject(application, 15000) == 0, '本轮软件未正常退出'
        finally:
            KERNEL.CloseHandle(application)
    else:
        time.sleep(0.5)
        assert not application_windows(installation)
    wait('系统释放主程序文件占用', lambda: executable_released(installation), timeout=15)
    unknown = installation / '未知文件.txt'
    unknown.write_text('保留', encoding='utf-8')
    marker = b'installer-flow-user-shortcut'
    if not desktop:
        with DESKTOP.open('xb') as stream:
            stream.write(marker)
    uninstaller = case / 'uninstall-check.exe'
    shutil.copyfile(installation / 'uninstall.exe', uninstaller)
    uninstall = subprocess.Popen(f'"{uninstaller}" _?={installation}', env=env, startupinfo=startup)
    confirm = wait('卸载确认页准备完成', lambda: next((handle for handle, _ in windows(uninstall.pid)
        if text(handle).startswith('Dinuosi') and control_id(handle, 1)
        and any(USER.GetDlgCtrlID(child) == 1000 and text(child) == str(installation) for child in controls(handle))), None))
    directory = control_id(confirm, 1000, shown=False)
    location_label = control_id(confirm, 1029, shown=False)
    assert directory and location_label and not visible(directory, confirm) and not visible(location_label, confirm)
    if desktop and run:
        snapshot(confirm, case / 'uninstall.png')
    click(control_id(confirm, 1))
    wait('卸载完成', lambda: not registered() and not (installation / 'dinuosi.exe').exists())
    wait('卸载关闭按钮可用', lambda: control_id(confirm, 1) and USER.IsWindowEnabled(control_id(confirm, 1)))
    click(control_id(confirm, 1))
    assert uninstall.wait(timeout=15) == 0
    if desktop:
        assert not DESKTOP.exists()
    else:
        assert DESKTOP.read_bytes() == marker, '卸载删除了安装器未创建的同名桌面文件'
        DESKTOP.unlink()
    assert not START_MENU.exists()
    assert unknown.read_text(encoding='utf-8') == '保留' and proof.read_text(encoding='utf-8') == '用户数据保留'
    print(f'{language} desktop={desktop} run={run}: PASS', flush=True)
    result = {'language': language, 'desktop': desktop, 'run': run, 'case': str(case)}
    (case / 'result.json').write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
    return result


artifacts = json.loads((ROOT / 'target/distribution/build-artifacts.json').read_text(encoding='utf-8-sig'))
setup_record = next(item for item in artifacts['outputs'] if item['name'].endswith('-setup.exe'))
SETUP = ROOT / 'target/distribution' / setup_record['name']
DESKTOP = folder(0x10) / 'Dinuosi.lnk'
START_MENU = folder(0x02) / 'Dinuosi'
if __name__ == '__main__':
    QA.mkdir(parents=True)
    results = [verify_case(language, desktop, run) for language in ('zh-CN', 'en')
               for desktop, run in ((False, False), (True, False), (False, True), (True, True))]
    report = {'setup': str(SETUP), 'setup_sha256': setup_record['sha256'], 'cases': results}
    (QA / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')
    print(str(QA / 'report.json'), flush=True)
