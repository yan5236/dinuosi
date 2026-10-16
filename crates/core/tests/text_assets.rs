//! 艺术字像素、用户样式库和 SRT 交换的边界回归。
use dinuosi_core::{
    ProjectStore,
    captions::parse_srt,
    text::{Background, Shadow, Text},
    text_render,
    timeline::Timeline,
};

/// 投影、背景和描边在共享像素中真实生效；关闭效果恢复原字形，预设值校验拒绝非法范围。
#[test]
fn editable_text_effects_produce_pixels_and_validate() {
    let mut text = Text::new("艺术字 Art".into(), (640, 360), false);
    text.style.size = 40.0;
    text.style.width = 300.0;
    let base = text_render::raster(&text, (640, 360)).unwrap();
    text.style.shadow = Some(Shadow {
        color: 0xff0000,
        opacity: 1.0,
        blur: 0,
        offset: [18, 18],
    });
    let shadow = text_render::raster(&text, (640, 360)).unwrap();
    assert!(
        shadow
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[2] > 100 && p[0] == 0 && p[1] == 0)
    );
    text.style.background = Some(Background {
        color: 0x0000ff,
        opacity: 0.5,
        padding: 14,
        radius: 0,
    });
    let background = text_render::raster(&text, (640, 360)).unwrap();
    assert!(
        background
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] > 200 && (120..=130).contains(&p[3]))
            .count()
            > 100
    );
    text.style.background.as_mut().unwrap().radius = 12;
    let rounded = text_render::raster(&text, (640, 360)).unwrap();
    assert_ne!(rounded.pixels, background.pixels);
    text.style.background = None;
    text.style.shadow.as_mut().unwrap().blur = 5;
    let blurred = text_render::raster(&text, (640, 360)).unwrap();
    assert!(
        blurred
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[2] > 100 && p[0] == 0 && p[3] > 0 && p[3] < 100)
    );
    text.style.shadow = None;
    assert_eq!(
        text_render::raster(&text, (640, 360)).unwrap().pixels,
        base.pixels
    );
    text.style.outline = 3;
    text.style.outline_color = 0x00ff00;
    assert!(
        text_render::raster(&text, (640, 360))
            .unwrap()
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[1] > 100 && p[2] == 0)
    );
    text.style.shadow = Some(Shadow {
        opacity: f64::NAN,
        ..Default::default()
    });
    assert!(text.validate().is_err());
}

/// 样式库跨 ProjectStore 重开保持完整效果；重名、非法名称和非法数值不覆盖已有预设。
#[test]
fn named_presets_persist_without_capturing_body_or_timing() {
    let root = std::env::temp_dir().join(format!("dinuosi-style-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let mut style = Text::new("不应保存的正文".into(), (1920, 1080), true).style;
    style.shadow = Some(Default::default());
    style.background = Some(Default::default());
    style.outline_color = 0xff0000;
    assert!(store.load_text_presets().unwrap().is_empty());
    let saved = store.save_text_preset("常用艺术字", style.clone()).unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].style, style);
    let fresh = ProjectStore::new(root.join("recent.json"));
    assert_eq!(fresh.load_text_presets().unwrap(), saved);
    assert!(store.save_text_preset("常用艺术字", style.clone()).is_err());
    assert!(store.save_text_preset(" ", style.clone()).is_err());
    style.size = f64::INFINITY;
    assert!(store.save_text_preset("坏样式", style).is_err());
    assert_eq!(fresh.load_text_presets().unwrap(), saved);
    let json = std::fs::read_to_string(root.join("text-presets.json")).unwrap();
    assert!(!json.contains("不应保存的正文"));
    assert!(!json.contains("group"));
    assert!(!json.contains("start"));
    assert!(fresh.remove_text_preset(&saved[0].id).unwrap().is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

/// UTF-8/BOM/CRLF、多行、重叠和常用标签能整体导入；导出再导入保持整数帧和正文。
#[test]
fn srt_import_export_preserves_content_group_and_frame_times() {
    let source = "\u{feff}7\r\n00:00:01,000 --> 00:00:03,000\r\n<b>第一句</b> &amp; Caption\r\n第二行\r\n\r\n12\r\n00:00:02,500 --> 00:00:04,100\r\n&lt;i&gt;原文标签&lt;/i&gt;\r\n";
    let mut t = Timeline::default();
    let ids = t.import_srt(source).unwrap();
    assert_eq!(ids.len(), 2);
    assert_eq!(t.clips[0].start, 30);
    assert_eq!(t.clips[0].duration, 60);
    assert_eq!(t.clips[1].start, 75);
    assert_eq!(t.clips[1].duration, 48);
    assert_ne!(t.clips[0].track, t.clips[1].track);
    let group = t.clips[0].text.as_ref().unwrap().group.as_ref().unwrap();
    assert_eq!(
        t.clips[0].text.as_ref().unwrap().body,
        "第一句 & Caption\n第二行"
    );
    assert_eq!(
        t.clips[1].text.as_ref().unwrap().group.as_ref(),
        Some(group)
    );
    let output = t.export_srt(group).unwrap();
    let cues = parse_srt(&output).unwrap();
    assert_eq!(cues[1].body, "<i>原文标签</i>");
    let mut reloaded = Timeline::default();
    reloaded.import_srt(&output).unwrap();
    for (a, b) in t.clips.iter().zip(reloaded.clips) {
        assert_eq!((a.start, a.duration), (b.start, b.duration));
        assert_eq!(a.text.as_ref().unwrap().body, b.text.unwrap().body);
    }
}

/// 错误在任何轨道写入前整体拒绝；短于一帧的有效句仍有一帧，极长数字不溢出。
#[test]
fn malformed_srt_is_atomic_and_tiny_cues_remain_editable() {
    let mut t = Timeline::default();
    let before = t.clone();
    for source in [
        "",
        "1\n00:00:02,000 --> 00:00:01,000\n坏时间",
        "1\n00:60:00,000 --> 01:00:01,000\n坏分钟",
        "1\n00:00:00,000 --> 99:00:00,000\n超长",
        "1\n00:00:00,000 --> 00:00:01,000\n有效\n\n2\nnot a timestamp\n错误",
    ] {
        assert!(t.import_srt(source).is_err());
        assert_eq!(t, before);
    }
    t.import_srt("1\n00:00:00,001 --> 00:00:00,002\n极短句")
        .unwrap();
    assert_eq!(t.clips[0].duration, 1);
}
