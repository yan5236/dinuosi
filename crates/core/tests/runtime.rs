use dinuosi_core::media;
use std::{fs, path::Path, process::Command};

/// 隔离程序验收入口：从复制后的程序位置加载动态库，禁止工具依赖和 PATH 回退。
/// CLI 已移除时仍需真实探测、寻址、播放及导出；缺桥接库或 libav 时断言失败诊断。
#[test]
fn installed_runtime_child() {
    let Ok(mode) = std::env::var("DINUOSI_RUNTIME_TEST") else {
        return;
    };
    let fixture = std::env::var_os("DINUOSI_RUNTIME_FIXTURE").unwrap();
    let result = media::probe(Path::new(&fixture));
    if mode == "missing" || mode == "dll" {
        assert!(result.err().unwrap().contains("动态库"));
    } else {
        let item = result.unwrap();
        assert_eq!(item.kind, media::MediaKind::Video);
        assert!(item.has_audio);
        let frame = media::frame(&item.path, 0.0).unwrap();
        assert_eq!(frame.len(), 640 * 360 * 4);
        assert!(frame.as_chunks::<4>().0.iter().any(|pixel| pixel[2] > 200));
        let mut decoder = dinuosi_core::media_stream::Decoder::start(&item, 0.0, 0.2).unwrap();
        let samples: Vec<_> = decoder.take_audio().unwrap().collect();
        assert_eq!(samples.len(), 19_200);
        assert!(samples.iter().any(|value| value.abs() > 0.03));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while decoder.frames.lock().unwrap().is_none() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        drop(decoder);
        let mut timeline = dinuosi_core::timeline::Timeline::default();
        let frames = timeline.fps as i64;
        timeline
            .insert(
                (item.path.clone(), item.kind),
                item.name.clone(),
                timeline.main_track(),
                0,
                0..frames,
                frames,
            )
            .unwrap();
        let mut settings = dinuosi_core::export::ExportSettings::for_timeline(
            &timeline,
            dinuosi_core::export::Encoder::Mpeg4,
        );
        settings.width = 64;
        settings.height = 64;
        let output = Path::new(&fixture).with_file_name("动态库导出.mp4");
        dinuosi_core::export::export(
            &timeline,
            &[item],
            &settings,
            &output,
            &std::sync::atomic::AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let exported = media::probe(&output).unwrap();
        assert!(exported.has_audio);
        assert!((exported.duration - 1.0).abs() < 0.05);
        assert_eq!(exported.source_size, (64, 64));
    }
}

/// 在中文空格路径复制真实测试程序与 runtime，改变工作目录并在 PATH 放置另一份工具。
/// 移除 CLI 后仍需完整媒体操作成功，再移除桥接库或 libav 确认失败；只改变子进程环境。
/// 通过后清理专属临时目录，失败保留现场；测试程序本身不在本进程更改环境变量。
#[test]
#[ignore = "需要准备本平台固定媒体运行时"]
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
    let child = app.join(format!("验收{}", std::env::consts::EXE_SUFFIX));
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
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-c:a",
                "aac",
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
    for name in ["ffmpeg", "ffprobe"] {
        let tool = runtime.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        fs::rename(&tool, runtime.join(format!("{name}.hidden"))).unwrap();
    }
    run("complete");
    let bridge = runtime.join(dinuosi_media::library_name());
    fs::rename(&bridge, runtime.join("bridge.hidden")).unwrap();
    run("missing");
    fs::rename(runtime.join("bridge.hidden"), &bridge).unwrap();
    let codec = if cfg!(target_os = "windows") {
        "avcodec-62.dll"
    } else if cfg!(target_os = "macos") {
        "libavcodec.62.dylib"
    } else {
        "libavcodec.so.62"
    };
    fs::rename(runtime.join(codec), runtime.join("avcodec.hidden")).unwrap();
    run("dll");
    fs::remove_dir_all(root).unwrap();
}
