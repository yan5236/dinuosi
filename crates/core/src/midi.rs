//! 标准 MIDI 使用内置 GM 采样库合成；与普通媒体输出同样的立体声 48kHz PCM。

use rustysynth::{MidiFile, MidiFileSequencer, SoundFont, Synthesizer, SynthesizerSettings};
use std::{
    fs::File,
    io::Cursor,
    path::Path,
    sync::{Arc, OnceLock},
};

const RATE: usize = 48000;
const BLOCK: usize = 64;
static SOUNDFONT: OnceLock<Result<Arc<SoundFont>, String>> = OnceLock::new();

/// 按扩展名识别标准 MIDI；大小写不敏感，不依赖会被用户重命名的素材显示名称。
pub(crate) fn is_midi(path: &Path) -> bool {
    path.extension().is_some_and(|e| {
        matches!(
            e.to_string_lossy().to_ascii_lowercase().as_str(),
            "mid" | "midi"
        )
    })
}

/// 解析标准 MIDI 事件与变速信息；损坏文件返回错误，不回退到缺失音源的 ModPlug。
fn read(path: &Path) -> Result<Arc<MidiFile>, String> {
    let mut file = File::open(path).map_err(|e| format!("无法读取 MIDI：{e}"))?;
    MidiFile::new(&mut file)
        .map(Arc::new)
        .map_err(|e| format!("MIDI 文件无效：{e}"))
}

/// 返回由 MIDI 事件及速度变化决定的真实长度；空曲或非有限长度返回可显示错误。
pub(crate) fn duration(path: &Path) -> Result<f64, String> {
    let duration = read(path)?.get_length();
    if duration.is_finite() && duration > 0.0 {
        Ok(duration)
    } else {
        Err("MIDI 没有有效播放时长".into())
    }
}

pub(crate) struct MidiPcm {
    sequencer: MidiFileSequencer,
    left: [f32; BLOCK],
    right: [f32; BLOCK],
    cursor: usize,
    available: usize,
    remaining: usize,
}

impl MidiPcm {
    /// 初始化采样合成并寻址到 offset 秒，区间结束于 end 秒；共享内置音源，不需要用户配置。
    /// 以固定小缓冲预渲染至寻址点，保留延音、控制器及音符包络；导出可在每个寻址块取消。
    /// cancel 为 None 时维持播放语义；不缓存整段音频或写入源文件，取消返回固定导出状态。
    /// ponytail: 寻址成本随起点增长；若长 MIDI 寻址成为瓶颈再增加音频缓存或状态快照。
    pub(crate) fn start(
        path: &Path,
        offset: f64,
        end: f64,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<Self, String> {
        if cancel.is_some_and(|cancel| cancel.load(std::sync::atomic::Ordering::Relaxed)) {
            return Err("导出已取消".into());
        }
        let midi = read(path)?;
        if !offset.is_finite()
            || !end.is_finite()
            || offset < 0.0
            || end <= offset
            || end > midi.get_length()
        {
            return Err("MIDI 播放区间无效".into());
        }
        let font = SOUNDFONT
            .get_or_init(|| {
                SoundFont::new(&mut Cursor::new(
                    include_bytes!("../../../assets/audio/FluidR3_GM.sf2").as_slice(),
                ))
                .map(Arc::new)
                .map_err(|e| format!("内置 MIDI 音源无效：{e}"))
            })
            .clone()?;
        let mut settings = SynthesizerSettings::new(RATE as i32);
        settings.maximum_polyphony = 256;
        let synth =
            Synthesizer::new(&font, &settings).map_err(|e| format!("无法初始化 MIDI 合成：{e}"))?;
        let mut sequencer = MidiFileSequencer::new(synth);
        sequencer.play(&midi, false);
        let mut pcm = Self {
            sequencer,
            left: [0.0; BLOCK],
            right: [0.0; BLOCK],
            cursor: 0,
            available: 0,
            remaining: ((end * RATE as f64).round() - (offset * RATE as f64).round()) as usize * 2,
        };
        let mut skip = (offset * RATE as f64).round() as usize;
        while skip > 0 {
            if cancel.is_some_and(|cancel| cancel.load(std::sync::atomic::Ordering::Relaxed)) {
                return Err("导出已取消".into());
            }
            let count = skip.min(BLOCK);
            pcm.sequencer
                .render(&mut pcm.left[..count], &mut pcm.right[..count]);
            skip -= count;
        }
        Ok(pcm)
    }
}

impl Iterator for MidiPcm {
    type Item = f32;
    /// 分块合成并交错返回左右声道；精确在选区末尾结束，限制输出峰值以免密集和弦过载。
    fn next(&mut self) -> Option<f32> {
        if self.remaining == 0 {
            return None;
        }
        if self.cursor == self.available {
            let count = (self.remaining / 2).min(BLOCK);
            self.sequencer
                .render(&mut self.left[..count], &mut self.right[..count]);
            self.cursor = 0;
            self.available = count * 2;
        }
        let sample = if self.cursor.is_multiple_of(2) {
            self.left[self.cursor / 2]
        } else {
            self.right[self.cursor / 2]
        };
        self.cursor += 1;
        self.remaining -= 1;
        Some(sample.clamp(-1.0, 1.0))
    }
}
