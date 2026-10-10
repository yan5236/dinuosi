# MIDI 音源

`FluidR3_GM.sf2` 为未经修改的 Fluid Release 3 General MIDI 音源，使用 RustySynth 编译嵌入 Core。用户无需安装音源；所有 GM 乐器及打击乐保留。

固定来源：Debian `fluid-soundfont-gm` 3.1-5.3。音源采用 MIT 许可，版权与原始许可文本见 `FluidR3-LICENSE.txt`。Debian 打包脚本的 GPL 许可不适用于所提取的音源文件，打包脚本不随应用分发。

音源约 141.5 MiB，不进入 Git；开发前运行 `tools/prepare-media.ps1`，按 `distribution/inputs.lock.json` 下载并验证原始 Debian 包及提取文件。音源变化会改变 MIDI 音色，不能承诺与旧音源或其他播放器逐样本一致。

旧 TimGM6mb 及仅属于它的 GPL 文件已从当前版本移除，不改写历史。主程序发布许可证与第三方资源许可分别管理。