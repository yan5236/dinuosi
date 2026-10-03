# MIDI 音源

`TimGM6mb.sf2` 为未修改的 TimGM6mb General MIDI 采样库，供 RustySynth 合成标准 MIDI。编译嵌入 Core，无需用户安装音源或 FFmpeg MIDI 插件。

来源：[Debian timgm6mb-soundfont 1.3-5](https://deb.debian.org/debian/pool/main/t/timgm6mb-soundfont/timgm6mb-soundfont_1.3-5_all.deb)，上游：[MuseScore 原始资源](https://github.com/musescore/musescore-old/commit/90c33ef9d87b3f5ff92efd3b07d89eb455fb1fef)。原始署名与资源许可在 `TimGM6mb-LICENSE.txt`，完整 GPL-2 文本在 `GPL-2.0.txt`；资源许可证与尚未确定的软件发布许可证分别管理。

保留全部 GM 乐器与打击乐，不替换 MIDI 中的乐器、音高、速度或控制器。不同播放器所用音源会影响音色；当前内置音源不承诺与 PotPlayer 默认音源逐样本一致。
