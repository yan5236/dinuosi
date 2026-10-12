//! 源帧缓存生命周期及像素快路径的行为回归；真实解码检查需显式启用 FFmpeg。
use super::*;

/// 对透明、半透明及不透明源像素验证旧混合公式，包含零透明度、越界平移、缩放和旋转。
#[test]
fn blend_fast_paths_preserve_pixels() {
    let source: Vec<_> = (0..FRAME_WIDTH * FRAME_HEIGHT)
        .flat_map(|i| [40, 120, 200, [0, 128, 255][i as usize % 3]])
        .collect();
    for opacity in [0.0, 37.0, 100.0] {
        for (scale, rotation, x, y) in [(100.0, 0.0, 0.0, 0.0), (57.0, 32.0, 360.0, -90.0)] {
            let t = Transform {
                opacity,
                scale,
                rotation,
                x,
                y,
                ..Default::default()
            };
            let mut actual = [90, 80, 70, 50].repeat((FRAME_WIDTH * FRAME_HEIGHT) as usize);
            let mut expected = actual.clone();
            let (sin, cos) = t.rotation.to_radians().sin_cos();
            for y in 0..FRAME_HEIGHT {
                for x in 0..FRAME_WIDTH {
                    let dx = x as f64 + 0.5 - 320.0 - t.x / 3.0;
                    let dy = y as f64 + 0.5 - 180.0 - t.y / 3.0;
                    let sx = ((dx * cos + dy * sin) / (t.scale / 100.0) + 320.0).floor() as i32;
                    let sy = ((-dx * sin + dy * cos) / (t.scale / 100.0) + 180.0).floor() as i32;
                    if !(0..640).contains(&sx) || !(0..360).contains(&sy) {
                        continue;
                    }
                    let i = ((y * FRAME_WIDTH + x) * 4) as usize;
                    let j = ((sy * 640 + sx) * 4) as usize;
                    let a = source[j + 3] as f64 / 255.0 * t.opacity / 100.0;
                    for c in 0..3 {
                        expected[i + c] = (source[j + c] as f64 * a
                            + expected[i + c] as f64 * (1.0 - a))
                            .round() as u8;
                    }
                    expected[i + 3] = 255;
                }
            }
            blend(&mut actual, &source, &t, (1920, 1080)).unwrap();
            assert_eq!(actual, expected);
        }
    }
}

/// 生成真实视频并暂时移走测试源文件，证明参数更新复用源帧；寻址、源入点、fps、
/// 重新加载、离线、隐藏和帧尾必须失效。逐帧寻址缓存保持一个源帧，失败后可恢复。
#[test]
#[ignore = "需要准备应用配套 FFmpeg 和 ffprobe"]
fn cached_transform_reuses_decode_and_invalidates_source_changes() {
    let root = std::env::temp_dir().join(format!("dinuosi-cache-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("video.mp4");
    let parked = root.join("parked.mp4");
    assert!(
        media::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=30",
                "-t",
                "2",
                "-c:v",
                "mpeg4"
            ])
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let item = media::probe(&path).unwrap();
    let mut timeline = Timeline::default();
    timeline
        .insert(
            (item.path.clone(), MediaKind::Video),
            "video".into(),
            0,
            0,
            0..30,
            60,
        )
        .unwrap();
    let media = std::slice::from_ref(&item);
    let mut cache = FrameCache::default();
    for frame in 0..6 {
        assert_eq!(
            cache.render(&timeline, frame, media).unwrap(),
            render(&timeline, frame, media).unwrap()
        );
        assert_eq!(cache.frames.len(), 1);
    }
    cache.render(&timeline, 0, media).unwrap();
    let mut transformed = timeline.clone();
    transformed.clips[0].transform = Transform {
        opacity: 43.0,
        scale: 65.0,
        x: 150.0,
        y: -60.0,
        rotation: 28.0,
        ..Default::default()
    };
    let expected = render(&transformed, 0, media).unwrap();
    std::fs::rename(&path, &parked).unwrap();
    assert_eq!(cache.render(&transformed, 0, media).unwrap(), expected);
    assert!(
        render(&transformed, 0, media).is_err(),
        "未缓存路径必须实际访问已移走的源文件"
    );
    std::fs::rename(&parked, &path).unwrap();
    for change in 0..7 {
        cache.render(&timeline, 0, media).unwrap();
        std::fs::rename(&path, &parked).unwrap();
        let mut next = timeline.clone();
        let mut source = item.clone();
        let frame = if change == 0 { 1 } else { 0 };
        match change {
            1 => next.clips[0].source_in = 1,
            2 => next.fps = 60,
            3 => {
                source.thumbnail = Some(Arc::new(item.thumbnail.as_ref().unwrap().as_ref().clone()))
            }
            4 => source.offline = true,
            5 => next.tracks[0].visible = false,
            6 => next.clips[0].start = 10,
            _ => {}
        }
        let result = cache.render(&next, frame, &[source]);
        if change < 4 {
            assert!(result.is_err(), "变更 {change} 必须重新解码");
        } else {
            assert!(
                result
                    .unwrap()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|p| *p == [0, 0, 0, 255])
            );
        }
        assert!(cache.frames.is_empty());
        std::fs::rename(&parked, &path).unwrap();
        assert!(
            cache.render(&timeline, 0, media).is_ok(),
            "失败/离线后可恢复"
        );
    }
    assert!(
        cache
            .render(&timeline, 30, media)
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [0, 0, 0, 255])
    );
    assert!(cache.frames.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
