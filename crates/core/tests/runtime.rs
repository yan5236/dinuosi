use dinuosi_core::media;
use std::{fs, path::Path, process::Command};

/// 子进程验收入口：只使用复制后的程序位置查找工具，禁止从 PATH 或当前目录补找。
/// 未设置验收环境变量时直接返回；父测试指定场景时断言版本、解码结果或缺失工具/DLL反馈。
#[test]
fn installed_runtime_child() {
    let Ok(mode) = std::env::var("DINUOSI_RUNTIME_TEST") else {
        return;
    };
    let output = media::command("ffprobe").arg("-version").output();
    if mode == "missing" {
        assert_eq!(output.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    } else if mode == "dll" {
        let output = output.unwrap();
        assert!(!output.status.success());
    } else {
        let output = output.unwrap();
        assert!(output.status.success());
        let version = String::from_utf8_lossy(&output.stdout);
        assert!(version.starts_with("ffprobe version 8.0.3"), "{version}");
        let fixture = std::env::var_os("DINUOSI_RUNTIME_FIXTURE").unwrap();
        let item = media::probe(Path::new(&fixture)).unwrap();
        assert_eq!(item.kind, media::MediaKind::Video);
        assert_eq!(media::frame(&item.path, 0.0).unwrap().len(), 640 * 360 * 4);
    }
}

/// 在中文空格路径复制真实测试程序与 runtime，改变工作目录并在 PATH 放置另一份工具。
/// 实际解码后移除配套工具/DLL确认失败，只改变子进程环境；全部断言通过后清理临时目录，失败保留现场。
#[test]
#[ignore = "需要 tools/prepare-media.ps1 准备固定运行时"]
fn relocated_runtime_ignores_path_and_reports_missing_files() {
    let exe = std::env::current_exe().unwrap();
    let source = exe.parent().unwrap().join("runtime/ffmpeg");
    let root = std::env::temp_dir().join(format!("迪诺斯 验收 {}", uuid::Uuid::new_v4()));
    let app = root.join("应用 目录");
    let runtime = app.join("runtime/ffmpeg");
    let unrelated = root.join("无关 工作目录");
    fs::create_dir_all(&runtime).unwrap();
    fs::create_dir(&unrelated).unwrap();
    for entry in fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), runtime.join(entry.file_name())).unwrap();
    }
    let child = app.join("验收.exe");
    fs::copy(exe, &child).unwrap();
    // PATH 中提供可启动的探测器，配套文件缺失仍必须失败，不能静默借用它。
    for entry in fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), unrelated.join(entry.file_name())).unwrap();
    }
    let fixture = root.join("视频.mp4");
    assert!(
        media::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=red:s=64x64:d=1",
                "-c:v",
                "mpeg4"
            ])
            .arg(&fixture)
            .status()
            .unwrap()
            .success()
    );
    let run = |mode: &str| {
        let output = Command::new(&child)
            .args(["--exact", "installed_runtime_child", "--nocapture"])
            .current_dir(&unrelated)
            .env("PATH", &unrelated)
            .env("DINUOSI_RUNTIME_TEST", mode)
            .env("DINUOSI_RUNTIME_FIXTURE", &fixture)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run("complete");
    fs::rename(runtime.join("ffprobe.exe"), runtime.join("ffprobe.hidden")).unwrap();
    run("missing");
    fs::rename(runtime.join("ffprobe.hidden"), runtime.join("ffprobe.exe")).unwrap();
    fs::rename(
        runtime.join("avcodec-62.dll"),
        runtime.join("avcodec.hidden"),
    )
    .unwrap();
    run("dll");
    fs::remove_dir_all(root).unwrap();
}
