"""检查 EXE 的全部品牌图标帧及真实 GPUI 窗口图标；仅 Windows，需 Pillow，默认验证 debug。"""
import ctypes as c
from ctypes import wintypes as w
import importlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import time

from PIL import Image, ImageGrab

ROOT = Path(__file__).resolve().parent.parent
KERNEL = c.WinDLL("kernel32", use_last_error=True)
USER = c.WinDLL("user32", use_last_error=True)
GDI = c.WinDLL("gdi32", use_last_error=True)
KERNEL.LoadLibraryExW.argtypes = [w.LPCWSTR, w.HANDLE, w.DWORD]
KERNEL.LoadLibraryExW.restype = w.HMODULE
KERNEL.FindResourceW.argtypes = [w.HMODULE, c.c_void_p, c.c_void_p]
KERNEL.FindResourceW.restype = w.HANDLE
KERNEL.LoadResource.argtypes = [w.HMODULE, w.HANDLE]
KERNEL.LoadResource.restype = w.HANDLE
KERNEL.LockResource.argtypes = [w.HANDLE]
KERNEL.LockResource.restype = c.c_void_p
KERNEL.SizeofResource.argtypes = [w.HMODULE, w.HANDLE]
KERNEL.FreeLibrary.argtypes = [w.HMODULE]
USER.LoadImageW.argtypes = [w.HMODULE, c.c_void_p, w.UINT, c.c_int, c.c_int, w.UINT]
USER.LoadImageW.restype = w.HANDLE
USER.GetClassLongPtrW.argtypes = [w.HWND, c.c_int]
USER.GetClassLongPtrW.restype = c.c_void_p
USER.DestroyIcon.argtypes = [w.HANDLE]
USER.FindWindowW.argtypes = [w.LPCWSTR, w.LPCWSTR]
USER.FindWindowW.restype = w.HWND
USER.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
USER.DrawIconEx.argtypes = [w.HDC, c.c_int, c.c_int, w.HANDLE, c.c_int, c.c_int, w.UINT, w.HBRUSH, w.UINT]
GDI.CreateCompatibleDC.argtypes = [w.HDC]
GDI.CreateCompatibleDC.restype = w.HDC
GDI.CreateDIBSection.argtypes = [w.HDC, c.c_void_p, w.UINT, c.POINTER(c.c_void_p), w.HANDLE, w.DWORD]
GDI.CreateDIBSection.restype = w.HBITMAP
GDI.SelectObject.argtypes = [w.HDC, w.HGDIOBJ]
GDI.SelectObject.restype = w.HGDIOBJ
GDI.DeleteObject.argtypes = [w.HGDIOBJ]
GDI.DeleteDC.argtypes = [w.HDC]


def resource(module, kind, identifier):
    """按整数类型与编号读取已加载 EXE 资源字节；资源缺失或加载失败时断言失败，不执行程序代码。"""
    entry = KERNEL.FindResourceW(module, identifier, kind)
    assert entry, f"资源缺失：type={kind}, id={identifier}"
    handle = KERNEL.LoadResource(module, entry)
    pointer = KERNEL.LockResource(handle)
    assert pointer, "资源加载失败"
    return c.string_at(pointer, KERNEL.SizeofResource(module, entry))


def render_icon(handle):
    """将原生 HICON 绘制为 32px BGRA 字节，用于比较真实窗口与 EXE 图标；释放本次 GDI 对象。

    不销毁调用方图标；绘制失败时断言失败，返回带透明通道的像素缓冲。
    """
    dc = GDI.CreateCompatibleDC(None)
    header = c.create_string_buffer(struct.pack("<IiiHHIIiiII", 40, 32, -32, 1, 32, 0, 4096, 0, 0, 0, 0))
    bits = c.c_void_p()
    bitmap = GDI.CreateDIBSection(dc, header, 0, c.byref(bits), None, 0)
    assert dc and bitmap and bits.value, "创建图标画布失败"
    previous = GDI.SelectObject(dc, bitmap)
    try:
        c.memset(bits, 0, 4096)
        assert USER.DrawIconEx(dc, 0, 0, handle, 32, 32, 0, None, 3), "绘制原生图标失败"
        return c.string_at(bits, 4096)
    finally:
        GDI.SelectObject(dc, previous)
        GDI.DeleteObject(bitmap)
        GDI.DeleteDC(dc)


def verify():
    """验证指定 EXE（默认 debug）资源与品牌 ICO 逐帧相同，再启动隔离窗口检查系统类图标。

    证据写入 .local/windows-icon/<构建目录>；只关闭本次进程，失败抛出异常，不访问用户项目。
    """
    executable = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "target/debug/dinuosi.exe"
    evidence = ROOT / ".local/windows-icon" / executable.parent.name
    evidence.mkdir(parents=True, exist_ok=True)
    module = KERNEL.LoadLibraryExW(str(executable), None, 2)
    assert module, f"读取 EXE 失败：{c.get_last_error()}"
    expected_icon = None
    try:
        ico = (ROOT / "assets/brand/dinuosi.ico").read_bytes()
        group = resource(module, 14, 1)
        assert group[:6] == ico[:6], "EXE 图标组与 ICO 不同"
        sizes = []
        for index in range(struct.unpack_from("<H", ico, 4)[0]):
            source = ico[6 + index * 16:22 + index * 16]
            entry = group[6 + index * 14:20 + index * 14]
            # RC 将 PNG 帧省略的 planes=0 规范化为 1，其余规格和完整 PNG 字节应保留。
            assert source[:4] + source[6:12] == entry[:4] + entry[6:12], "EXE 图标帧规格改变"
            assert struct.unpack_from("<H", entry, 4)[0] == 1, "EXE 图标 planes 非法"
            length, offset = struct.unpack_from("<II", source, 8)
            identifier = struct.unpack_from("<H", entry, 12)[0]
            assert resource(module, 3, identifier) == ico[offset:offset + length], "EXE 图标像素改变"
            sizes.append(source[0] or 256)
        expected_icon = USER.LoadImageW(module, 1, 1, 32, 32, 0)
        assert expected_icon, "EXE 图标无法加载"
        expected = render_icon(expected_icon)
        env = os.environ.copy()
        env["DINUOSI_DATA_DIR"] = str(evidence / "user-data")
        with (evidence / "startup.log").open("wb") as log:
            process = subprocess.Popen([str(executable)], env=env, stdout=log, stderr=log, cwd=evidence)
            try:
                time.sleep(8)
                assert process.poll() is None, "程序提前退出"
                window = int(importlib.import_module("verify-desktop-startup").window_id(process))
                handle = USER.GetClassLongPtrW(window, -14)
                assert handle, "原生窗口未设置图标"
                pixels = render_icon(handle)
                assert pixels == expected, "真实窗口图标与 EXE 品牌图标不同"
                Image.frombytes("RGBA", (32, 32), pixels, "raw", "BGRA").save(evidence / "window-icon.png")
                report = {"executable": str(executable), "resource_id": 1, "sizes": sizes,
                          "resource_bytes_match": True, "window_id": window, "window_icon_matches": True}
                tray = USER.FindWindowW("Shell_TrayWnd", None)
                bounds = w.RECT()
                if tray and USER.GetWindowRect(tray, c.byref(bounds)):
                    ImageGrab.grab(bbox=(bounds.left, bounds.top, bounds.right, bounds.bottom),
                                   all_screens=True).save(evidence / "taskbar.png")
                (evidence / "result.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
                print(json.dumps(report))
            finally:
                if process.poll() is None:
                    process.terminate()
                process.wait(timeout=10)
    finally:
        if expected_icon:
            USER.DestroyIcon(expected_icon)
        KERNEL.FreeLibrary(module)


if __name__ == "__main__":
    verify()
