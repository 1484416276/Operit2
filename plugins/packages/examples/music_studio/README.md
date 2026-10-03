# Operit Music Studio / 音乐工作台

AI 优先的 external 插件。TypeScript + 自包含 HTML/WebView；音频由原生 Web Audio 合成，不包含录音采样、SoundFont、在线生成服务或运行时 CDN。

## 默认试听：ECLIPSE · 逐光

原创 D minor / 150 BPM / 4⁄4 melodic dubstep 展示工程：48 小节、19 轨、约 1700 个音符事件，音乐 76.8 秒。包含氛围 Intro、渐进 Trance Build、两个不同的 Drop、Breakdown、二次蓄力和 Outro。详见 [曲目设计](docs/ECLIPSE.md)。

首次打开自动创建；**已有工程不会被示例覆盖**。已有用户从「音色库 → 工程模板 → ECLIPSE」创建一份新工程。默认全曲 FIT 总览；缩放、段落跳转和钢琴卷帘用于检查局部。

## 安装与开发

在此目录执行：

```sh
npm ci
npm run pack:toolpkg
# 导入 dist/music_studio.toolpkg 并启用插件
npm run preview
# 在终端给出的 localhost 地址预览
```

宿主入口：侧边栏插件 / 工具箱中的「音乐工作台」。首次点击播放解锁音频。网页预览保存到浏览器 localStorage；宿主模式保存到 PluginConfig，由主运行时统一管理。两者数据不混用。

```sh
npm run typecheck
npm test
npm run test:browser  # 需要本机 Google Chrome；先 npm run build
```

浏览器测试检查六种视口、抽屉/编辑器切换、播放不中断、段落 seek、循环边界、真正 L/R 电平，以及完整离线 WAV 的声部数、峰值、立体声和段落能量。测试生成 `.test-output/eclipse.wav` 与分析 JSON（不打包、不入库）。这些是自动化检查，不代表 Android/iOS 真机性能或主观听感已验收。

## 功能

- 5 种合成引擎：双振荡器/谐波波形、FM、合成乐器层、程序鼓机、立体声环境噪声；30 个原创预置。
- 7 类效果器：三段 EQ、低通、过载、立体声 Chorus、节拍/乒乓 Delay、程序生成立体声混响、压缩。
- 声部宽度、滤波包络/LFO、音高上升；轨道音量、声像、静音/独奏。
- 每轨 level / pan / cutoff 自动化；示例把底鼓位置编译成音量避让包络，**不是通用外部 sidechain 输入检测器**。
- 时间线、钢琴卷帘、鼓步进、总线限制器、实际双声道电平与频谱、节拍器、工程 JSON 和 PCM16 立体声 WAV 导出。
- 7 个 AI 子包、32 个工具，支持原子编曲事务、版本冲突检查、撤销/重做；详见 [AI 接口](docs/AI_TOOLS.md)。

默认关闭音色库、总控和编辑器；宽屏单侧停靠，窄屏抽屉/底部面板，移动端编辑器替换编排视图。只有时间线或当前面板内部滚动，不堆叠多个整页栏目。

## 工程布局

```text
manifest.json          ToolPkg 注册、子包和资源声明
src/main.ts            插件入口、IPC 与导航
src/api.ts             公共 API
src/service.ts         唯一持久状态、事务、命令回执
src/host-service.ts    PluginConfig 适配
src/packages/          7 组 AI 工具和 IPC client
src/shared/            模型、校验、音色、自动化、编曲与模板
src/ui/screen.ts        宿主 WebView 桥接
web/audio/             实时/离线共用合成器与效果器
web/ui/                DAW 布局与 Canvas 绘制
web/app.ts              交互、同步、播放和导出
scripts/               构建、打包、预览
resources/             自包含 HTML 与声明
dist/                 编译 JS 和 .toolpkg（归档不入库）
tests/                 模型、服务、音频和响应式测试
docs/                  AI 接口和示例编曲说明
```

## 边界与性能

- 24 轨、每轨最多 6000 音符、每轨 6 效果、64 个并发音符声部（unison 内部振荡器另计）、128 小节、30 个工程。
- WAV 音乐部分最多 90 秒，额外固定 3.2 秒尾音；长反馈延迟/混响可能超过尾音窗口，不自动无限延长。44.1 kHz / 双声道 / PCM16。
- 实时前瞻调度，进入后台暂停；关闭 WebView 后不继续后台播放。输出硬件和宿主 WebView 必须支持标准 Web Audio 节点。
- 离线渲染按 4 秒窗口创建声部，保留效果器状态，减少未来节点占用；耗时取决于设备。不是实时录音。
- 当前没有引入 WASM：DSP 由浏览器原生音频节点执行，JS 负责控制。未实现 VST/AU 宿主、Kontakt 采样兼容、商业 Serum 音色加载、音频录音、MIDI 硬件或母带响度认证。
- 音色是自主简化设计，不是商业合成器的完整复刻。无第三方旋律、预置或音频素材。
