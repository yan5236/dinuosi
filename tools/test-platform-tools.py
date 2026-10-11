"""检查跨平台部署的真实文件边界：四个 Cargo 路径、权限保留和漂移拒绝，不下载或构建媒体库。"""
import importlib.util
import json
from pathlib import Path
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def self_check():
    """用临时源码及运行库清单执行实际部署，篡改运行库和输入后断言拒绝，不触碰用户媒体或真实缓存。"""
    spec = importlib.util.spec_from_file_location("prepare", ROOT / "tools/prepare-media-unix.py")
    prepare = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(prepare)
    with tempfile.TemporaryDirectory(prefix="dinuosi-platform-tools-") as temporary:
        prepare.ROOT = Path(temporary)
        prepare.WORK = prepare.ROOT / "work"
        prepare.LOCK = prepare.ROOT / "inputs.lock.json"
        runtime = prepare.WORK / "runtime"
        runtime.mkdir(parents=True)
        font = prepare.ROOT / "assets/audio/FluidR3_GM.sf2"
        font.parent.mkdir(parents=True)
        font.write_bytes(b"locked-font")
        prepare.LOCK.write_text(json.dumps({"soundfontSha256": prepare.digest(font)}))
        executable = runtime / "ffmpeg"
        executable.write_bytes(b"locked-runtime")
        executable.chmod(0o755)
        manifest = {"build_inputs": {prepare.LOCK.name: prepare.digest(prepare.LOCK)},
            "files": [{"name": "ffmpeg", "sha256": prepare.digest(executable)}]}
        (prepare.WORK / "runtime-manifest.json").write_text(json.dumps(manifest))
        # 隔离 Cargo 输出，测试不依赖调用者设置的 target 路径。
        import os
        old_target = os.environ.get("CARGO_TARGET_DIR")
        os.environ["CARGO_TARGET_DIR"] = str(prepare.ROOT / "target")
        try:
            prepare.deploy()
            for profile in ("debug", "debug/deps", "release", "release/deps"):
                copied = prepare.ROOT / "target" / profile / "runtime/ffmpeg/ffmpeg"
                assert copied.read_bytes() == b"locked-runtime"
                assert copied.stat().st_mode == executable.stat().st_mode
            for changed in (executable, prepare.LOCK):
                original = changed.read_bytes()
                changed.write_bytes(original + b"tampered")
                try:
                    prepare.deploy()
                except ValueError:
                    pass
                else:
                    raise AssertionError(f"漂移未被拒绝：{changed}")
                changed.write_bytes(original)
        finally:
            if old_target is None:
                os.environ.pop("CARGO_TARGET_DIR", None)
            else:
                os.environ["CARGO_TARGET_DIR"] = old_target
    print("平台部署路径、权限及运行库/构建输入漂移拒绝通过。")


if __name__ == "__main__":
    self_check()
