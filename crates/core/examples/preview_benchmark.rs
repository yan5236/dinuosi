//! 用真实视频及叠加图片测量暂停时的连续变换；运行参数为视频与图片路径。
use dinuosi_core::{
    composite, media,
    timeline::{Timeline, TrackKind},
};
use std::{hint::black_box, time::Instant};

/// 加载指定素材并报告 40 次位置、缩放、旋转更新的中位数和 P95；失败直接返回错误。
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("需要视频和图片路径".into());
    }
    let sources = args
        .iter()
        .map(|p| media::probe(p.as_ref()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut timeline = Timeline::default();
    timeline.create_track(0, TrackKind::Media)?;
    for (index, source) in sources.iter().enumerate() {
        timeline.insert(
            (source.path.clone(), source.kind),
            source.name.clone(),
            1 - index,
            0,
            0..150,
            150,
        )?;
    }
    for cached in [false, true] {
        let mut cache = composite::FrameCache::default();
        let mut samples = Vec::new();
        for i in 0..40 {
            let transform = &mut timeline.clips[1].transform;
            transform.x = i as f64 * 8.0;
            transform.scale = 45.0 + i as f64;
            transform.rotation = i as f64 * 2.0;
            let start = Instant::now();
            black_box(if cached {
                cache.render(&timeline, 30, &sources)?
            } else {
                composite::render(&timeline, 30, &sources)?
            });
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "cached={cached} median={:.2}ms p95={:.2}ms",
            samples[20], samples[38]
        );
    }
    Ok(())
}
