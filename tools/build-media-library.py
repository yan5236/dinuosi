"""用受控 FFmpeg 8 的头文件和共享库编译 Dinuosi C ABI 桥，不链接系统 FFmpeg。"""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent


def compiler_environment():
    """返回本机构建环境；Windows 沿用当前 MSVC 或读取已安装 x64 vcvars，不打印环境秘密。"""
    env = {key.upper(): value for key, value in os.environ.items()} if sys.platform == "win32" else os.environ.copy()
    if sys.platform != "win32" or shutil.which("cl.exe"):
        return env
    finder = Path(env["PROGRAMFILES(X86)"]) / "Microsoft Visual Studio/Installer/vswhere.exe"
    installed = subprocess.check_output([str(finder), "-latest", "-products", "*", "-requires",
        "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "installationPath"], text=True).strip()
    setup = Path(installed) / "VC/Auxiliary/Build/vcvars64.bat"
    if not setup.is_file() or any(c in str(setup) for c in '\n\r"%'):
        raise ValueError("无法定位有效 MSVC x64 环境")
    result = subprocess.run(f'cmd.exe /d /u /s /c ""{setup}" >nul && set"',
        check=True, capture_output=True)
    for line in result.stdout.decode("utf-16-le").splitlines():
        if "=" in line and not line.startswith("="):
            key, value = line.split("=", 1)
            env[key.upper()] = value
    return env


def build(prefix, sources, output):
    """将三个原生模块编译为单一可替换动态库；仅使用 prefix 的头文件/库，失败返回非零。

    Windows 从受控 DEF 生成导入库；Unix 为桥接库写相对加载路径，源码与生成对象分离。
    """
    prefix, sources, output = prefix.resolve(), sources.resolve(), output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    scratch = output.parent / "media-bridge-build"
    scratch.mkdir(parents=True, exist_ok=True)
    env = compiler_environment()
    files = [str(sources / name) for name in ("probe.c", "decode.c", "encode.c")]
    libraries = ("avformat", "avcodec", "avfilter", "avutil", "swscale")
    if sys.platform == "win32":
        librarian = shutil.which("lib.exe", path=env["PATH"])
        compiler = shutil.which("cl.exe", path=env["PATH"])
        if not librarian or not compiler:
            raise ValueError("MSVC x64 编译器或导入库工具缺失")
        for name in libraries:
            definitions = list((prefix / "lib").glob(f"{name}-*.def"))
            if len(definitions) != 1:
                raise ValueError(f"受控导入定义缺失或重复：{name}")
            subprocess.run([librarian, "/nologo", "/machine:x64", f"/def:{definitions[0]}",
                f"/name:{definitions[0].stem}.dll", f"/out:{scratch / (name + '.lib')}"], env=env, check=True)
        args = [compiler, "/nologo", "/utf-8", "/std:c11", "/O2", "/MT", "/W3", "/LD",
            "/D_CRT_SECURE_NO_WARNINGS", f"/I{prefix / 'include'}", *files,
            "/link", f"/OUT:{output / 'dinuosi_media.dll'}", f"/IMPLIB:{scratch / 'dinuosi_media.lib'}",
            f"/LIBPATH:{scratch}", *[f"{name}.lib" for name in libraries]]
    else:
        name = "libdinuosi_media.dylib" if sys.platform == "darwin" else "libdinuosi_media.so"
        args = ["cc", "-std=c11", "-O2", "-fPIC", "-fvisibility=hidden", "-Wall", "-Wextra", "-Werror",
            "-dynamiclib" if sys.platform == "darwin" else "-shared", f"-I{prefix / 'include'}", *files,
            f"-L{prefix / 'lib'}", *[f"-l{lib}" for lib in libraries], "-lm", "-o", str(output / name)]
        if sys.platform == "darwin":
            args += ["-Wl,-rpath,@loader_path", "-Wl,-install_name,@rpath/libdinuosi_media.dylib"]
        else:
            args += ["-Wl,-rpath,$ORIGIN", "-Wl,-z,origin"]
    subprocess.run(args, cwd=scratch, env=env, check=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--prefix", type=Path, required=True)
    default_sources = Path(__file__).resolve().parent / "media-native"
    parser.add_argument("--sources", type=Path, default=default_sources if default_sources.is_dir() else ROOT / "crates/media/native")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    build(args.prefix, args.sources, args.output or args.prefix / "bin")
