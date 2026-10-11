"""在 CI 原生桌面或 Linux 虚拟显示中启动真实程序，验证窗口创建并保存截图；不访问用户项目。"""
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
EVIDENCE = ROOT / ".local/ci-export"


def window_id(process):
    """从本机窗口系统查询指定进程的真实窗口，返回标识；未创建窗口或查询失败抛出异常。

    Linux 需 X11 工具，macOS 使用系统 Quartz 元数据，Windows 使用进程主窗口句柄；不模拟 GPUI。
    """
    if sys.platform == "win32":
        args = ["powershell", "-NoProfile", "-Command", f"(Get-Process -Id {process.pid}).MainWindowHandle"]
    elif sys.platform == "darwin":
        source = '''import Cocoa
let windows = CGWindowListCopyWindowInfo([.optionAll, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
guard let window = windows.first(where: { ($0[kCGWindowOwnerPID as String] as? Int) == Int(CommandLine.arguments.last!) && ($0[kCGWindowLayer as String] as? Int) == 0 }) else { exit(1) }
print(window[kCGWindowNumber as String]!)'''
        args = ["swift", "-e", source, str(process.pid)]
    else:
        output = subprocess.check_output(["xdotool", "search", "--onlyvisible", "--pid", str(process.pid)], text=True)
        return output.splitlines()[0]
    identifier = subprocess.check_output(args, text=True, timeout=60).strip()
    if not identifier or identifier == "0":
        raise RuntimeError("主窗口未创建")
    return identifier


def verify_linux_drag(identifier):
    """用 X11 原生鼠标事件拖动自绘标题空白区，并断言窗口实际移动 80×40 像素。

    只操作本次启动的验收窗口；不改变大小或项目，返回前后位置用于独立证据。
    """
    def geometry():
        """读取验收窗口的 X11 几何字段，查询失败即终止，避免仅凭鼠标事件成功判定拖动通过。"""
        raw = subprocess.check_output(["xdotool", "getwindowgeometry", "--shell", identifier], text=True)
        return {key: int(value) for key, value in (line.split("=", 1) for line in raw.splitlines())}
    initial = geometry()
    x, y = initial["X"] + initial["WIDTH"] // 2, initial["Y"] + 18
    for args in (["mousemove", "--sync", str(x), str(y)], ["mousedown", "1"],
                 ["mousemove", "--sync", str(x + 80), str(y + 40)], ["mouseup", "1"]):
        subprocess.run(["xdotool", *args], check=True)
        time.sleep(0.3)
    moved = geometry()
    assert abs(moved["X"] - initial["X"] - 80) <= 2, f"标题拖动 X 未生效：{initial} -> {moved}"
    assert abs(moved["Y"] - initial["Y"] - 40) <= 2, f"标题拖动 Y 未生效：{initial} -> {moved}"
    return {"initial": initial, "moved": moved}


def verify():
    """以独立数据目录启动当前 debug 程序，验证八秒后仍运行且存在真实窗口，并尝试保存屏幕证据。

    截图权限不足时在报告中明确记录；窗口不存在仍失败。仅终止本次创建的进程，不操作用户软件。
    """
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env["DINUOSI_DATA_DIR"] = str(EVIDENCE / "desktop-user-data")
    display_manager = None
    if sys.platform == "linux":
        display_manager = subprocess.Popen(["openbox"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    executable = ROOT / "target/debug" / ("dinuosi.exe" if sys.platform == "win32" else "dinuosi")
    with (EVIDENCE / "desktop-startup.log").open("wb") as log:
        process = subprocess.Popen([str(executable)], cwd=EVIDENCE, env=env, stdout=log, stderr=log)
        try:
            time.sleep(8)
            if process.poll() is not None:
                raise RuntimeError(f"桌面提前退出：{process.returncode}")
            identifier = window_id(process)
            drag = verify_linux_drag(identifier) if sys.platform == "linux" else None
            screenshot = EVIDENCE / "desktop-startup.png"
            if sys.platform == "darwin":
                args = ["screencapture", "-x", "-l", identifier, str(screenshot)]
            elif sys.platform == "linux":
                args = ["import", "-window", identifier, str(screenshot)]
            else:
                script = """$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bitmap = [System.Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
$bitmap.Save($args[0], [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()"""
                script_path = EVIDENCE / "capture.ps1"
                script_path.write_text(script, encoding="utf-8")
                args = ["powershell", "-NoProfile", "-File", str(script_path), str(screenshot)]
            result = subprocess.run(args, capture_output=True, timeout=60)
            report = {"window_id": identifier, "running": process.poll() is None,
                "window_drag": drag,
                "screenshot": "captured" if result.returncode == 0 and screenshot.exists() else "unavailable",
                "screenshot_error": result.stderr.decode(errors="replace")}
            (EVIDENCE / "desktop-startup.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
            assert report["running"]
            print(json.dumps(report), flush=True)
        finally:
            process.terminate()
            process.wait(timeout=30)
            if display_manager:
                display_manager.terminate()
                display_manager.wait(timeout=10)


if __name__ == "__main__":
    verify()
