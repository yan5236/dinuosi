"""从锁定源码准备 Linux/macOS 配套媒体运行库；不安装系统 FFmpeg，不使用 GPL 编码器。"""
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
WORK = ROOT / ".local/distribution/unix"
LOCK = ROOT / "distribution/inputs.lock.json"


def digest(path):
    """流式计算给定文件 SHA-256，避免将大型源码或运行库整体读入内存；读取失败向上抛出。"""
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def fetch(entry):
    """下载锁定输入到项目缓存并验证哈希，错误缓存或下载立即拒绝，不覆盖已验证归档。"""
    path = WORK / "downloads" / entry["file"]
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists():
        pending = path.with_suffix(path.suffix + ".pending")
        with urllib.request.urlopen(entry["url"], timeout=120) as source, pending.open("wb") as target:
            shutil.copyfileobj(source, target)
        if digest(pending) != entry["sha256"]:
            raise ValueError(f"下载哈希不符：{path.name}")
        pending.rename(path)
    if digest(path) != entry["sha256"]:
        raise ValueError(f"缓存哈希不符：{path.name}")
    return path


def run(args, cwd, env=None):
    """直接运行构建参数数组并实时输出日志；不经过 shell，非零退出立即停止后续部署。"""
    print("build:", " ".join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), cwd=cwd, env=env, check=True)


def deploy():
    """校验已构建运行库及构建输入，复制到 Cargo 当前输出路径的四个配置位置；缺失或漂移拒绝部署。"""
    manifest = json.loads((WORK / "runtime-manifest.json").read_text())
    lock = json.loads(LOCK.read_text())
    if digest(ROOT / "assets/audio/FluidR3_GM.sf2") != lock["soundfontSha256"]:
        raise ValueError("内嵌音源哈希不符")
    for relative, expected in manifest["build_inputs"].items():
        if digest(ROOT / relative) != expected:
            raise ValueError(f"构建输入已变更，请重新构建：{relative}")
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target.is_absolute():
        target = ROOT / target
    for entry in manifest["files"]:
        source = WORK / "runtime" / entry["name"]
        if digest(source) != entry["sha256"]:
            raise ValueError(f"运行库哈希不符：{source}")
        for profile in ("debug", "debug/deps", "release", "release/deps"):
            destination = target / profile / "runtime/ffmpeg"
            destination.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination / source.name)


def build():
    """验证源码哈希和官方签名后原生构建 LGPL FFmpeg、BSD 编码器与 zlib/LAME；只写项目缓存。

    Linux/macOS 用相对加载路径定位配套共享库，编码库静态进入独立 FFmpeg；不链接至主程序。
    完成真实版本/许可检查后记录哈希清单并部署，失败不会更新清单。
    """
    lock = json.loads(LOCK.read_text())
    inputs = {**lock["inputs"], **lock["unixInputs"]}
    source_keys = ("ffmpeg", "zlib", "lame", "aom", "vpx", "openh264")
    if platform.machine() == "x86_64":
        source_keys += ("nasmSource",)
    archives = {key: fetch(inputs[key]) for key in (*source_keys, "ffmpegKey", "ffmpegSignature")}
    gpg = WORK / "gpg"
    gpg.mkdir(parents=True, exist_ok=True, mode=0o700)
    run(["gpg", "--homedir", gpg, "--import", archives["ffmpegKey"]], ROOT)
    verified = subprocess.run(["gpg", "--homedir", str(gpg), "--status-fd", "1", "--verify",
        str(archives["ffmpegSignature"]), str(archives["ffmpeg"])], check=True, capture_output=True, text=True)
    if f"[GNUPG:] VALIDSIG {lock['ffmpegSigningKey']} " not in verified.stdout:
        raise ValueError("FFmpeg 签名身份不符")
    sources = {}
    for key in source_keys:
        folder = WORK / "sources" / key
        folder.mkdir(parents=True, exist_ok=True)
        with tarfile.open(archives[key]) as archive:
            archive.extractall(folder, filter="data")
        sources[key] = next(path for path in folder.iterdir() if path.is_dir())
    prefix = WORK / "codecs"
    prefix.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env["CFLAGS"] = "-O2 -fPIC"
    env["CXXFLAGS"] = "-O2 -fPIC"
    env["PKG_CONFIG_LIBDIR"] = str(prefix / "lib/pkgconfig")
    env.pop("PKG_CONFIG_PATH", None)
    jobs = os.environ.get("DINUOSI_BUILD_JOBS", "2")
    if "nasmSource" in sources:
        # libaom 3.13.1 的帮助文本检测不兼容 NASM 3；使用与 Windows 相同的锁定 2.16.03。
        tools = WORK / "tools"
        run(["./configure", f"--prefix={tools}"], sources["nasmSource"], env)
        run(["make", "-j", jobs, "install"], sources["nasmSource"], env)
        env["PATH"] = str(tools / "bin") + os.pathsep + env["PATH"]
    run(["./configure", f"--prefix={prefix}", "--static"], sources["zlib"], env)
    run(["make", "-j", jobs, "install"], sources["zlib"], env)
    run(["./configure", f"--prefix={prefix}", "--disable-shared", "--enable-static", "--with-pic", "--disable-frontend"], sources["lame"], env)
    run(["make", "-j", jobs, "install"], sources["lame"], env)
    aom_build = WORK / "build/aom"
    run(["cmake", "-S", sources["aom"], "-B", aom_build, "-DCMAKE_BUILD_TYPE=Release",
        f"-DCMAKE_INSTALL_PREFIX={prefix}", "-DCMAKE_INSTALL_LIBDIR=lib", "-DCMAKE_POSITION_INDEPENDENT_CODE=ON",
        "-DBUILD_SHARED_LIBS=OFF", "-DENABLE_NASM=ON", "-DENABLE_TESTS=OFF", "-DENABLE_EXAMPLES=OFF", "-DENABLE_TOOLS=OFF", "-DENABLE_DOCS=OFF"], ROOT, env)
    run(["cmake", "--build", aom_build, "--parallel", jobs], ROOT, env)
    run(["cmake", "--install", aom_build], ROOT, env)
    vpx_args = ["./configure", f"--prefix={prefix}", "--enable-pic", "--disable-shared",
        "--enable-static", "--disable-examples", "--disable-tools", "--disable-unit-tests", "--disable-docs"]
    if platform.machine() == "x86_64":
        vpx_args.append("--as=nasm")
    run(vpx_args, sources["vpx"], env)
    run(["make", "-j", jobs, "install"], sources["vpx"], env)
    # ARM 的 OpenH264 汇编依赖外部 gas-preprocessor，软件路径使用其自带 C 实现以保持自包含。
    asm = "Yes" if platform.machine() == "x86_64" else "No"
    run(["make", "-j", jobs, f"PREFIX={prefix}", "PIC=Yes", f"USE_ASM={asm}", "install-static"], sources["openh264"], env)
    runtime = WORK / "runtime"
    runtime.mkdir(parents=True, exist_ok=True)
    ffmpeg_build = WORK / "build/ffmpeg"
    ffmpeg_build.mkdir(parents=True, exist_ok=True)
    args = [sources["ffmpeg"] / "configure", f"--prefix={WORK / 'install'}",
        "--disable-autodetect", "--disable-gpl", "--disable-nonfree", "--disable-version3",
        "--enable-shared", "--disable-static", "--disable-ffplay", "--disable-doc", "--disable-debug",
        "--disable-network", "--enable-zlib", "--enable-libmp3lame", "--enable-libaom", "--enable-libvpx",
        "--enable-libopenh264", "--pkg-config-flags=--static", f"--extra-cflags=-I{prefix / 'include'}"]
    if sys.platform == "darwin":
        args.extend(["--enable-videotoolbox", "--install-name-dir=@rpath", f"--extra-ldflags=-L{prefix / 'lib'} -Wl,-rpath,@loader_path"])
    else:
        args.append(f"--extra-ldflags=-L{prefix / 'lib'}")
    run(args, ffmpeg_build, env)
    run(["make", "-j", jobs], ffmpeg_build, env)
    run(["make", "install"], ffmpeg_build, env)
    install = WORK / "install"
    for path in [install / "bin/ffmpeg", install / "bin/ffprobe", *list((install / "lib").iterdir())]:
        if path.is_file() and (path.name in ("ffmpeg", "ffprobe") or ".so" in path.name or path.suffix == ".dylib"):
            shutil.copy2(path, runtime / path.name)
    if sys.platform == "linux":
        # ELF 的 $ORIGIN 在 configure/make 多层展开中会丢失，部署后统一写入可迁移的相对加载路径。
        for path in sorted(runtime.iterdir()):
            run(["patchelf", "--set-rpath", "$ORIGIN", path], ROOT)
    version = subprocess.check_output([str(runtime / "ffmpeg"), "-version"], text=True)
    assert version.startswith(f"ffmpeg version {lock['ffmpegVersion']}")
    license_text = subprocess.check_output([str(runtime / "ffmpeg"), "-L"], text=True, stderr=subprocess.STDOUT)
    assert "GNU Lesser General Public" in license_text
    assert "--enable-gpl" not in version and "--enable-nonfree" not in version
    manifest = {"platform": platform.platform(), "ffmpegVersion": lock["ffmpegVersion"], "configuration": version,
        "build_inputs": {str(path.relative_to(ROOT)): digest(path) for path in (LOCK, Path(__file__).resolve())},
        "files": [{"name": path.name, "sha256": digest(path)} for path in sorted(runtime.iterdir()) if path.is_file()]}
    (WORK / "runtime-manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    deploy()


if __name__ == "__main__":
    if sys.platform not in ("linux", "darwin"):
        raise SystemExit("该脚本仅用于 Linux/macOS；Windows 使用 prepare-media.ps1")
    if sys.argv[1:] == ["--deploy"]:
        deploy()
    elif not sys.argv[1:]:
        from importlib.util import module_from_spec, spec_from_file_location
        spec = spec_from_file_location("verification", ROOT / "tools/verify-export-platform.py")
        verification = module_from_spec(spec)
        spec.loader.exec_module(verification)
        verification.EVIDENCE.mkdir(parents=True, exist_ok=True)
        verification.prepare_font()
        build()
    else:
        raise SystemExit("参数必须为空或 --deploy")
