"""使用现有 MSVC 与锁定 NSIS 3.13 的插件 SDK 构建安装详情按钮扩展，仅写发行构建缓存。"""
import importlib
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent


def build():
    """沿用现有编译环境定位 VS，再加载 x86 环境构建 Unicode NSIS 插件，静态使用 C 运行库。

    NSIS 安装器本身是 x86，与应用 x64 无关；缺少 SDK/工具或编译失败抛错，禁止复用旧 DLL。
    """
    env = importlib.import_module('build-media-library').compiler_environment()
    setup = Path(env['VSINSTALLDIR']) / 'VC/Auxiliary/Build/vcvars32.bat'
    if not setup.is_file() or any(char in str(setup) for char in '\n\r"%'):
        raise ValueError('无法定位有效 MSVC x86 环境')
    output = subprocess.run(f'cmd.exe /d /u /s /c ""{setup}" >nul && set"', check=True, capture_output=True)
    for line in output.stdout.decode('utf-16-le').splitlines():
        if '=' in line and not line.startswith('='):
            key, value = line.split('=', 1)
            env[key.upper()] = value
    work = ROOT / '.local/distribution/build/installer-ui'
    work.mkdir(parents=True, exist_ok=True)
    sdk = ROOT / '.local/distribution/build/nsis/nsis-3.13/Examples/Plugin'
    compiler = shutil.which('cl.exe', path=env['PATH'])
    if not compiler or not (sdk / 'nsis/pluginapi-x86-unicode.lib').is_file():
        raise ValueError('MSVC x86 编译器或锁定 NSIS 插件 SDK 缺失')
    # 锁定 SDK 的静态库带旧 LIBC 指令，忽略它并使用 /MT 选择的现代静态 CRT。
    subprocess.run([compiler, '/nologo', '/utf-8', '/std:c11', '/O2', '/MT', '/W4', '/WX', '/LD',
        f'/I{sdk}', str(ROOT / 'distribution/installer-ui.c'), '/link',
        f'/OUT:{work / "DinuosiInstaller.dll"}', '/NODEFAULTLIB:LIBC', str(sdk / 'nsis/pluginapi-x86-unicode.lib'),
        'user32.lib', 'comctl32.lib'], cwd=work, env=env, check=True)


if __name__ == '__main__':
    build()
