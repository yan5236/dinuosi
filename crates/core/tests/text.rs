//! 文字和字幕的领域、存储及共享像素验收；不依赖 GUI 或外部字体文件副本。
use dinuosi_core::{
    ProjectStore, composite,
    text::{Alignment, Text},
    text_render,
    timeline::{History, Timeline, TrackKind},
};

/// 在小画布上创建默认文字，保持测试工作量固定且中文字号足够辨认。
fn text(body: &str, subtitle: bool) -> Text {
    let mut text = Text::new(body.into(), (640, 360), subtitle);
    text.style.size = 28.0;
    text
}

/// 普通文字和字幕在不同帧率下按结束帧夹紧整组移动及右裁边，极值增量也不能生成不可保存模型。
#[test]
fn text_move_and_trim_respect_day_end() {
    for fps in [24, 30, 60] {
        for subtitle in [false, true] {
            let mut t = Timeline {
                fps,
                ..Default::default()
            };
            let end = 86400 * fps as i64;
            let a = t.add_text(end - 90, 90, text("末尾", subtitle)).unwrap();
            let b = t.add_text(end - 180, 30, text("前句", subtitle)).unwrap();
            let ids = [a.clone(), b.clone()].into();
            let before = t.clone();
            for delta in [1, i64::MAX] {
                t.move_clips(&ids, delta, 0).unwrap();
                t.trim(&a, false, delta).unwrap();
                t.trim_clips(&ids, false, delta).unwrap();
                assert_eq!(t, before);
                t.validate().unwrap();
            }
            t.move_clips(&ids, -30, 0).unwrap();
            t.move_clips(&ids, i64::MAX, 0).unwrap();
            assert_eq!(t, before);
            t.trim(&a, false, -30).unwrap();
            t.trim(&a, false, i64::MAX).unwrap();
            assert_eq!(t, before);
            t.move_clips(&ids, i64::MIN, 0).unwrap();
            assert_eq!(t.clips.iter().map(|c| c.start).min(), Some(0));
            assert_eq!(t.clips[0].start - t.clips[1].start, 90);
            t.trim(&a, true, i64::MIN).unwrap();
            assert_eq!(t.clips[0].start, 0);
            t.trim(&a, false, i64::MAX).unwrap();
            assert_eq!(t.clips[0].duration, end);
            t.validate().unwrap();
        }
    }
}

/// 九个带描边和阴影的短文本反复请求仍复用栅格，避免固定八项缓存使每帧重新排版绘制。
#[test]
fn nine_text_rasters_remain_cached_across_frames() {
    let canvas = (640, 360);
    let texts: Vec<_> = (0..9)
        .map(|i| {
            let mut t = text(&format!("缓存文本 {i}"), false);
            t.style.outline = 2;
            t.style.shadow = Some(Default::default());
            t
        })
        .collect();
    let rasters: Vec<_> = texts
        .iter()
        .map(|t| text_render::raster(t, canvas).unwrap())
        .collect();
    for _ in 0..10 {
        for (t, expected) in texts.iter().zip(&rasters) {
            assert!(std::sync::Arc::ptr_eq(
                expected,
                &text_render::raster(t, canvas).unwrap()
            ));
        }
    }
}

/// 重复添加不会丢失句子；重叠自动分轨，移动、裁边、切割保留文字及字幕组身份。
#[test]
fn text_placement_trim_split_and_history_preserve_content() {
    let mut t = Timeline {
        width: 640,
        height: 360,
        ..Default::default()
    };
    let mut history = History::default();
    let before = t.clone();
    let a = t.add_text(0, 90, text("标题 Title", false)).unwrap();
    let b = t.add_text(0, 90, text("第一句\nCaption", true)).unwrap();
    let group = t.clips[1].text.clone().unwrap();
    let c = t.add_text(30, 60, group.clone()).unwrap();
    assert_eq!(t.clips.len(), 3);
    assert_ne!(t.clips[1].track, t.clips[2].track);
    assert_eq!(t.tracks[t.clips[0].track].kind, TrackKind::Text);
    assert_eq!(t.tracks[t.clips[1].track].kind, TrackKind::Subtitle);
    t.move_clips(&[b.clone()].into(), 150, 0).unwrap();
    t.trim(&a, false, 600).unwrap();
    assert_eq!(t.clips[0].duration, 690);
    t.trim(&c, true, -15).unwrap();
    assert_eq!(t.clips[2].source_in, 0);
    assert_eq!(t.clips[2].text.as_ref(), Some(&group));
    t.split(&[a].into(), 30).unwrap();
    assert_eq!(t.clips.len(), 4);
    t.validate().unwrap();
    let after = t.clone();
    history.record(before.clone(), &after);
    history.step(&mut t, false);
    assert_eq!(t, before);
    history.step(&mut t, true);
    assert_eq!(t, after);
    t.configure(640, 360, 1, 12000).unwrap();
    t.validate().unwrap();
}

/// 字幕统一样式仅覆盖同组，正文和变换独立；单句修改及锁定拒绝均有可见业务结果。
#[test]
fn caption_style_scope_is_explicit_and_atomic() {
    let mut t = Timeline::default();
    let group = text("第一句", true);
    let a = t.add_text(0, 90, group.clone()).unwrap();
    let b = t.add_text(120, 90, group).unwrap();
    let c = t.add_text(0, 90, text("另一组", true)).unwrap();
    let mut style = t.clips[0].text.as_ref().unwrap().style.clone();
    style.size = 44.0;
    t.set_text_style(&a, style.clone(), true).unwrap();
    assert_eq!(t.clips[1].text.as_ref().unwrap().style, style);
    assert_ne!(t.clips[2].text.as_ref().unwrap().style.size, 44.0);
    t.set_text_body(&b, "第二句 Caption".into()).unwrap();
    assert_eq!(t.clips[0].text.as_ref().unwrap().body, "第一句");
    assert_eq!(t.clips[1].name, "第二句 Caption");
    style.size = 55.0;
    t.set_text_style(&a, style.clone(), false).unwrap();
    assert_eq!(t.clips[1].text.as_ref().unwrap().style.size, 44.0);
    t.clips[1].locked = true;
    let before = t.clone();
    style.size = 60.0;
    assert!(t.set_text_style(&a, style, true).is_err());
    assert_eq!(t, before);
    assert!(t.set_text_range(&b, 30, 60).is_err());
    assert_eq!(t, before);
    t.set_text_range(&c, 90, 120).unwrap();
    assert_eq!(t.clips[0].start, 0);
    assert_eq!(t.clips[1].start, 120);
}

/// 保存重开保留多行、字体、字幕组与样式；旧文件缺可选 text 字段仍可读取，损坏文字拒绝载入。
#[test]
fn text_round_trip_and_external_validation() {
    let root = std::env::temp_dir().join(format!("dinuosi-text-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("文字验证", &root).unwrap();
    let mut t = Timeline::default();
    let mut content = text("中文\nEnglish 😀", true);
    content.style.font = "Microsoft YaHei".into();
    content.style.alignment = Alignment::Right;
    t.add_text(30, 90, content).unwrap();
    store.save_timeline(&entry, &t).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), t);
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&entry.path).unwrap()).unwrap();
    json["timeline"]["clips"][0]["text"]["style"]["size"] = (-1).into();
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(store.load_timeline(&entry).is_err());
    t.clips.clear();
    t.remove_empty_tracks();
    store.save_timeline(&entry, &t).unwrap_err();
    std::fs::remove_dir_all(root).unwrap();
}

/// 正文、字体及数值在领域入口校验，失败不创建空轨道或部分改动；明确禁止文件接口伪造文字。
#[test]
fn malformed_text_edits_leave_timeline_unchanged() {
    let mut t = Timeline::default();
    let before = t.clone();
    assert!(t.add_text(-1, 90, text("A", false)).is_err());
    assert!(t.add_text(0, 0, text("A", false)).is_err());
    assert!(t.add_text(86400 * 30, 1, text("A", false)).is_err());
    assert!(t.add_text(0, 90, text(&"字".repeat(4097), false)).is_err());
    assert!(t.add_text(0, 90, text("A\0B", false)).is_err());
    assert_eq!(t, before);
    let a = t.add_text(0, 90, text("A", false)).unwrap();
    let mut style = t.clips[0].text.as_ref().unwrap().style.clone();
    style.size = f64::NAN;
    let before = t.clone();
    assert!(t.set_text_style(&a, style, false).is_err());
    assert!(t.set_text_range(&a, 60, 30).is_err());
    assert!(t.move_clips(&[a].into(), 0, 1).is_err());
    assert_eq!(t, before);
}

/// 多行、自动折行、中文和拉丁字形输出真实非透明像素；缓存命中且正文/样式修改会失效。
#[test]
fn shaping_wrap_fallback_outline_and_cache_have_real_pixels() {
    let canvas = (640, 360);
    let base = text("中文 English", false);
    let first = text_render::raster(&base, canvas).unwrap();
    assert!(
        first
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 0)
            .count()
            > 300
    );
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &text_render::raster(&base, canvas).unwrap()
    ));
    let chinese = text_render::raster(&text("中文", false), canvas).unwrap();
    let latin = text_render::raster(&text("English", false), canvas).unwrap();
    assert_ne!(chinese.pixels, latin.pixels);
    let mut multi = base.clone();
    multi.body = "第一行\nSecond line".into();
    let explicit = text_render::raster(&multi, canvas).unwrap();
    assert!(explicit.size.1 > first.size.1);
    multi.body = "中文长句 English text wraps over several lines".into();
    multi.style.width = 130.0;
    assert!(text_render::raster(&multi, canvas).unwrap().size.1 > first.size.1);
    let mut outlined = base.clone();
    outlined.style.outline = 2;
    outlined.style.color = 0xff0000;
    let outlined = text_render::raster(&outlined, canvas).unwrap();
    assert!(
        outlined
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[2] > 200 && p[0] == 0 && p[3] > 100)
    );
    assert!(
        outlined
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] == 0 && p[1] == 0 && p[2] == 0 && p[3] > 100)
    );
}

/// 预览遵守片段起闭末开区间、隐藏和画布坐标；纯文字项目不需要素材引用或运行库。
#[test]
fn text_only_preview_respects_time_visibility_and_transform() {
    let mut t = Timeline {
        width: 640,
        height: 360,
        ..Default::default()
    };
    t.add_text(30, 60, text("中文 Title", false)).unwrap();
    let blank = composite::render(&t, 29, &[]).unwrap();
    assert!(
        blank
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [0, 0, 0, 255])
    );
    let visible = composite::render(&t, 30, &[]).unwrap();
    assert_ne!(visible, blank);
    assert_eq!(composite::render(&t, 90, &[]).unwrap(), blank);
    t.clips[0].transform.x = 100.0;
    assert_ne!(composite::render(&t, 30, &[]).unwrap(), visible);
    t.tracks[t.clips[0].track].visible = false;
    assert_eq!(composite::render(&t, 30, &[]).unwrap(), blank);
}

/// 真实动态库编码中文斜体纯文字项目，再共享解码读取输出，确认两个时段的文字确实烧录进视频。
/// 产物保留在项目本地验收目录，依赖已部署的原生媒体运行库，默认忽略避免冒充普通单测。
#[test]
#[ignore = "需要已部署原生媒体运行库"]
fn real_text_only_video_export_contains_burned_in_glyphs() {
    use dinuosi_core::{
        export::{self, Encoder, ExportSettings},
        media,
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/text-qa/export");
    std::fs::create_dir_all(&root).unwrap();
    let output = root.join(format!("text-{}.mp4", uuid::Uuid::new_v4()));
    let mut t = Timeline {
        width: 640,
        height: 360,
        ..Default::default()
    };
    let mut title = text("中文标题 Title 01", false);
    title.style.italic = true;
    title.style.color = 0xff0000;
    title.style.shadow = Some(Default::default());
    title.style.background = Some(Default::default());
    t.add_text(0, 15, title).unwrap();
    t.add_text(15, 15, text("手动字幕\nCaption 02", true))
        .unwrap();
    let settings = ExportSettings::for_timeline(&t, Encoder::Mpeg4);
    export::export(&t, &[], &settings, &output, &Default::default(), |_| {}).unwrap();
    let info = media::probe(&output).unwrap();
    assert!((info.duration - 1.0).abs() < 0.1);
    let first = media::frame(&output, 0.1).unwrap();
    let second = media::frame(&output, 0.7).unwrap();
    assert!(
        first
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[2] > 120 && p[0] < 50)
            .count()
            > 300
    );
    assert!(
        second
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] > 180 && p[1] > 180 && p[2] > 180)
            .count()
            > 300
    );
    assert_ne!(first, second);
    println!("真实文本导出：{}", output.display());
}
