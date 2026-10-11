"""CI 平台诊断：三平台均使用锁定源码构建的配套运行库，保留真实导出与部署证据。"""
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
EVIDENCE = ROOT / ".local/ci-export"


def run(name, args, *, data=None, timeout=900):
    """在仓库执行参数数组并保留完整诊断；返回退出码及标准输出，启动失败/超时记为失败。

    不经过 shell，不更改调用者环境；日志只写忽略目录，单项失败不阻止后续独立检查。
    """
    try:
        result = subprocess.run(args, cwd=ROOT, input=data, capture_output=True, timeout=timeout)
        code, output, error = result.returncode, result.stdout, result.stderr
    except (OSError, subprocess.TimeoutExpired) as failure:
        code, output, error = 1, b"", str(failure).encode()
    (EVIDENCE / f"{name}.log").write_bytes(output + b"\n" + error)
    print(f"{name}: {'PASS' if code == 0 else 'FAIL'}", flush=True)
    return code, output


def prepare_font():
    """下载并验证仓库锁定音源包，用 ar/tar 提取；包或音源哈希不符立即拒绝编译。

    仅写项目忽略目录与既有内嵌音源路径，不安装系统音源，不改变输入锁。
    """
    lock = json.loads((ROOT / "distribution/inputs.lock.json").read_text())
    entry = lock["inputs"]["soundfont"]
    archive = EVIDENCE / entry["file"]
    with urllib.request.urlopen(entry["url"], timeout=120) as response:
        with archive.open("wb") as target:
            shutil.copyfileobj(response, target)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != entry["sha256"]:
        raise ValueError("音源包哈希不符")
    subprocess.run(["ar", "x", str(archive)], cwd=EVIDENCE, check=True)
    subprocess.run(["tar", "-xf", "data.tar.xz", "./usr/share/sounds/sf2/FluidR3_GM.sf2"],
                   cwd=EVIDENCE, check=True)
    font = EVIDENCE / "usr/share/sounds/sf2/FluidR3_GM.sf2"
    if hashlib.sha256(font.read_bytes()).hexdigest() != lock["soundfontSha256"]:
        raise ValueError("内嵌音源哈希不符")
    shutil.copyfile(font, ROOT / "assets/audio/FluidR3_GM.sf2")
    font.unlink()
    archive.unlink()
    (EVIDENCE / "data.tar.xz").unlink()


def validate_streams(probe, codec):
    """断言真实封装文件的视频编码、尺寸、帧率、AAC 声道/采样率及一秒时长。

    输入为 ffprobe JSON 与预期编码名；缺流、字段错误或时长偏差均抛出断言错误。
    """
    video = next(s for s in probe["streams"] if s["codec_type"] == "video")
    audio = next(s for s in probe["streams"] if s["codec_type"] == "audio")
    assert video["codec_name"] == codec
    assert (video["width"], video["height"], video["r_frame_rate"]) == (320, 240, "24/1")
    assert (audio["codec_name"], audio["sample_rate"], audio["channels"]) == ("aac", "48000", 2)
    assert abs(float(probe["format"]["duration"]) - 1) < 0.05


def tool_exports(ffmpeg, ffprobe):
    """独立试编码当前平台各实现的 MP4/MKV，保存样片、探测与失败诊断。

    这是媒体工具环境对照，不调用应用合成器，也不把未接入的系统 H.264 后端算作支持。
    返回每个实现/封装的验证状态；未知编码器及无 GPU 是明确失败，其他组合继续执行。
    """
    outcomes = {}
    h264 = [
        ("h264-hardware", "h264_mf", "h264", ["-hw_encoding", "1"]),
        ("h264-software", "h264_mf", "h264", ["-hw_encoding", "0"]),
    ] if sys.platform == "win32" else [
        ("h264-openh264", "libopenh264", "h264", []),
    ]
    if sys.platform == "darwin":
        h264 = [
            ("h264-hardware", "h264_videotoolbox", "h264", ["-allow_sw", "0"]),
            ("h264-software", "h264_videotoolbox", "h264", ["-allow_sw", "1", "-require_sw", "1"]),
        ] + h264
    for label, encoder, codec, options in h264 + [
        ("av1", "libaom-av1", "av1", ["-cpu-used", "6", "-row-mt", "1"]),
        ("vp9", "libvpx-vp9", "vp9", ["-deadline", "good", "-cpu-used", "4", "-row-mt", "1"]),
        ("mpeg4", "mpeg4", "mpeg4", []),
    ]:
        for container in ("mp4", "mkv"):
            name = f"tool-{label}-{container}"
            output = EVIDENCE / f"{name}.{container}"
            code, _ = run(name, [str(ffmpeg), "-v", "error", "-y", "-threads", "1",
                "-filter_threads", "1", "-f", "rawvideo", "-pixel_format", "bgra",
                "-video_size", "320x240", "-framerate", "24", "-i", "pipe:0",
                "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=1",
                "-c:v", encoder, "-pix_fmt", "nv12" if encoder in ("h264_mf", "h264_videotoolbox") else "yuv420p",
                "-b:v", "3000k", "-threads", "2", *options,
                "-c:a", "aac", "-b:a", "192k", "-ac", "2", "-t", "1", str(output)],
                data=bytes([0, 0, 255, 255]) * (320 * 240 * 24), timeout=120)
            if code == 0:
                code, raw = run(f"{name}-probe", [str(ffprobe), "-v", "error",
                    "-show_streams", "-show_format", "-of", "json", str(output)])
                if code == 0:
                    try:
                        validate_streams(json.loads(raw), codec)
                    except (AssertionError, ValueError, KeyError, StopIteration):
                        code = 1
            outcomes[name] = "passed" if code == 0 else "failed"
    return outcomes


def verify():
    """运行共享模块测试、媒体回归及真实导出，再生成区分应用/工具的 JSON 和 CI 摘要。

    三平台都要求配套受控运行库存在，不复制系统 FFmpeg；失败返回非零，保留全部独立检查结果。
    桌面构建与事件测试由工作流另行执行；工具试编码不能代表应用时间线或实机 GPU 验收。
    """
    runtime = ROOT / "target/debug/deps/runtime/ffmpeg"
    if sys.platform == "win32":
        ffmpeg, ffprobe = runtime / "ffmpeg.exe", runtime / "ffprobe.exe"
        origin = "locked source-built Windows runtime"
    else:
        ffmpeg, ffprobe = runtime / "ffmpeg", runtime / "ffprobe"
        origin = "locked source-built Unix runtime"
    run("ffmpeg-version", [str(ffmpeg), "-version"])
    run("ffmpeg-configuration", [str(ffmpeg), "-buildconf"])
    run("ffmpeg-encoders", [str(ffmpeg), "-encoders"])
    outcomes = {}
    for name, args in [
        ("core-tests", ["cargo", "test", "-p", "dinuosi-core", "-p", "dinuosi-command", "-p", "dinuosi-icons", "--locked", "--", "--test-threads=1"]),
        ("application-media-and-export", ["cargo", "test", "-p", "dinuosi-core", "--locked",
            "--", "--ignored", "--nocapture", "--test-threads=1", "--skip", "real_midi_preview_samples"]),
    ]:
        code, _ = run(name, args)
        outcomes[name] = "passed" if code == 0 else "failed"
    tools = tool_exports(ffmpeg, ffprobe)
    report = {"commit": os.environ.get("GITHUB_SHA"), "platform": platform.platform(),
        "machine": platform.machine(), "runtime_origin": origin,
        "application_checks": outcomes, "tool_only_checks": tools}
    (EVIDENCE / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    summary = f"## {platform.system()} / {platform.machine()}\n\nRuntime: {origin}\n\n"
    summary += "| Check | Result |\n| --- | --- |\n"
    summary += "".join(f"| {name} | {value} |\n" for name, value in {**outcomes, **tools}.items())
    summary += "\nTool checks are not application or release certification. Physical GPU acceptance remains separate.\n"
    (EVIDENCE / "summary.md").write_text(summary, encoding="utf-8")
    if path := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(path, "a", encoding="utf-8") as target:
            target.write(summary)
    return int("failed" in outcomes.values())


def self_check():
    """验证探测断言既接受正确输出也拒绝错误尺寸，避免脚本只凭编码进程成功判定通过。"""
    probe = {"streams": [
        {"codec_type": "video", "codec_name": "vp9", "width": 320, "height": 240, "r_frame_rate": "24/1"},
        {"codec_type": "audio", "codec_name": "aac", "sample_rate": "48000", "channels": 2}],
        "format": {"duration": "1.01"}}
    validate_streams(probe, "vp9")
    probe["streams"][0]["width"] = 128
    try:
        validate_streams(probe, "vp9")
    except AssertionError:
        code, _ = run("self-check-missing-command", [str(EVIDENCE / "nonexistent-ci-command")])
        assert code != 0
        return
    raise AssertionError("错误尺寸未被拒绝")


if __name__ == "__main__":
    os.chdir(ROOT)
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    self_check()
    if sys.argv[1] == "prepare-font":
        prepare_font()
    elif sys.argv[1] == "verify":
        sys.exit(verify())
    elif sys.argv[1] != "self-check":
        raise ValueError("参数必须为 prepare-font、verify 或 self-check")
