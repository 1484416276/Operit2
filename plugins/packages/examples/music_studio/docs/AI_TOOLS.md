# AI 编曲接口

## 必须先读工程

1. `music_project.get` 获取当前工程、实际 `project.id` / `revision`、轨道/效果器/音符 ID。
2. `music_project.catalog` 获取音色、效果参数范围、模板、合成器与自动化范围、资源上限。
3. 每次修改携带 **projectId + revision**。成功后用返回的新 revision；冲突时重新 get 并重新理解修改意图，不盲重放旧批次。
4. `patch / notes / lanes / operations / options` 等标注 string 的参数必须是 **JSON 字符串**；内部数字/布尔值仍用 JSON 原生类型。

## 子包

| 子包 | 工具 |
| --- | --- |
| `music_project` | get, catalog, create, open, duplicate, import_project, export_project, undo, redo, delete_project |
| `music_tracks` | add, set, remove |
| `music_notes` | set, add, remove, transform |
| `music_instruments` | catalog, preset, design |
| `music_effects` | catalog, add, set, remove |
| `music_arrangement` | configure, generate_pattern, sections, automation, batch |
| `music_transport` | status, control, render_wav |

以 `src/packages/*.ts` 中的 METADATA 为实际参数签名来源。`music_project.create` 的 template 可为 `eclipse`、`neon-drive`、`ambient-orbit`、`midnight-keys`；不指定则新建空白工程。初次安装展示工程则默认为 Eclipse。

## 时间、音高与增益

- `start / duration / beat` 是从 0 开始的**四分音符拍**，不是秒或小节。4/4 的第 17 小节起点为 64 拍。
- 音高 MIDI 0..127，力度 0.01..1，音符时长至少 0.01 拍，不得越过工程结尾。
- `gain` 是线性轨道增益，`pan` 为 -1..1；自动化 level 是轨道效果之后的额外倍率。
- 空白工程默认循环；Eclipse 默认完整 song mode。
- 不要把每拍/每个音符拆成工具调用，用 batch 或一批 notes。最多 100 个操作，整批原子验证，失败不保存部分结果。

## 示例：一笔事务创建音色与渐进包络

传给 `music_arrangement.batch` 的 `operations` 是下列数组序列化后的字符串；另传当前 `projectId` / `revision`。以下示例要求工程至少 32 拍：

```json
[
  {"type":"track.add","id":"ai_arp","preset":"prism-arp","name":"AI · Ascending Prism"},
  {"type":"pattern.generate","trackId":"ai_arp","options":{"kind":"arpeggio","start":0,"bars":8,"root":62,"scale":"minor","velocity":0.65,"seed":17}},
  {"type":"effect.add","trackId":"ai_arp","effectType":"delay","mix":0.2,"params":{"beats":0.75,"feedback":0.28,"pingPong":1}},
  {"type":"automation.set","trackId":"ai_arp","lanes":[
    {"target":"cutoff","points":[{"beat":0,"value":800},{"beat":24,"value":5000},{"beat":32,"value":14000}]},
    {"target":"level","points":[{"beat":0,"value":0.3},{"beat":31,"value":1},{"beat":31.75,"value":0}]}
  ]}
]
```

`automation.set` / `music_arrangement.automation` **替换该轨道全部自动化**，不是追加；传 `[]` 清除。

- 最多 3 条，不可重复 target。
- 每条 1..1024 点，beat 必须严格递增且位于工程内。
- level 0..1.5；pan -1..1（绝对声像，覆盖静态 pan）；cutoff 40..18000 Hz。
- 相邻点线性插值，首点前/末点后保持边界值。
- 设置 level=0 是后效果器静音；想保留混响尾音，请改音符/合成器包络，而不是后级硬静音。
- 缩短工程必须同一事务处理越界 notes、sections、automation 和 loop，否则整笔拒绝。

## 合成与效果

`music_instruments.design` 接受部分 synth patch。新增参数：`width` 0..1、`filterEnv` -6..6 八度、`lfoRate` 0..16 Hz、`lfoDepth` 0..3 八度、`pitchSweep` -48..48 半音。filterEnv 为负时随音符时长渐开，为正时从高频衰减；LFO 调制声部滤波，pitchSweep 在音符内滑向相对终点。

`atmosphere` 是运行时噪声/弱谐波环境，不是音频文件。`ensemble` 是合成弦/管等近似音色，不是采样乐器。FM 只有两算子。低频建议 width=0、detune=0、unison=1，并与宽中高频分层。

Effect 链按数组顺序串联。delay 的 `pingPong >= 0.5` 开启左右交替；并非连续宽度控制。所有具体参数从 catalog 查询，未知键会报错。

## 播放与导出不是即时成功

工具可以在界面关闭时编曲，但播放/渲染要求 WebView 已打开；play 还要求用户先点击一次播放完成解锁。

`control` / `render_wav` 返回 queued 后，轮询 `status` 的 receipts，只有 `done` 和真实保存路径才表示导出成功。渲染期间修改工程会作废该命令。渲染命令最多等待 10 分钟，其余命令 2 分钟；设备速度与页面可见性影响完成时间。

WAV 为 44.1 kHz PCM16 双声道，包含自动化和效果，不含节拍器；长度限制和文件位置见 README。不要把 JSON 导出冒充音频，也不要承诺专业响度/真机性能而未测量。
