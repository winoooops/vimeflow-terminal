<!-- Modified from herdr by the vimeflow project — see FORK.md -->

# vimeflow-terminal

**[herdr](https://github.com/herdrdev/herdr) 的一个有主见的下游分叉。**

<p align="center">
  <a href="https://github.com/herdrdev/herdr">上游 herdr</a> ·
  <a href="https://herdr.dev/docs/">herdr 文档</a> ·
  <a href="FORK.md">本分叉改了什么</a> ·
  <a href="#试一试">试一试</a>
</p>

<p align="center">
  <a href="README.md">English</a> · 简体中文 · <a href="README.ja.md">日本語</a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-666666?labelColor=333333" alt="Apache 2.0 license" /></a>
</p>

---

## 先试试 herdr

**如果你还没用过 [herdr](https://herdr.dev)，请先从它开始。** herdr 才是正主：
一个住在终端里的智能体多路复用器——所有智能体的状态一目了然，随时随地分离与
重连，智能体自己就能驱动的纯 socket API，键盘与鼠标同等一流，一个 Rust 二进制。

```bash
curl -fsSL https://herdr.dev/install.sh | sh
```

herdr 可安装、有文档、按计划发布、有人维护。本分叉目前这些都还没有。你想要的
几乎所有东西 herdr 都已经有了——而且做得更好，因为上游在正式发布它。

只有当你已经用过 herdr，并且明确想要下面描述的这层"有主见"的功能时，再回到
这里。

## 本分叉是什么

[Vimeflow](https://github.com/winoooops/vimeflow) 曾是一个面向编码智能体的
Electron 桌面应用。2026 年 8 月它转向终端原生，以 **herdr v0.8.0 的跟踪分叉**
的形式重建。

分工是刻意的：

- **herdr 提供引擎**——PTY 归属、通过供应（vendored）的 libghostty-vt 实现的
  VT 仿真、workspace → tab → pane 模型、git 与工作树状态、智能体检测、通知、
  插件系统、socket API。
- **vimeflow 在其上提供一层有主见的功能**——把智能体可观测性做成内置的界面
  元素而非可选插件，以及一组假定你同时运行多个编码智能体的工作流功能。

最简短的描述：**有主见的 herdr**。上游保持通用；本分叉替你做选择。如果这里
的某个选择被证明对所有人都是对的，它就应该进上游，而不是留在分叉里。

这是一个*跟踪*分叉，不是硬分叉。上游发布按版本合并进来，对上游文件的每一处
修改都有记录，目标是保持可合并——而不是渐行渐远。

## 现在有什么

herdr v0.8.0 的全部功能，加上：

- **内置智能体观察器**——编码智能体的可观测性（转录监视、生命周期、指标、
  通知）编译进二进制，随服务端一起运行。不需要安装、链接或同步任何插件。命令行
  在 `vimeflow watcher` 下。
- **自动 pane 标题**——pane 标签随智能体工作时的会话标题变化。手动重命名永远
  优先，绝不会被覆盖。
- **Agents 侧边栏卡片**——侧边栏的 Agents 区渲染智能体卡片（生命周期加上
  下文、缓存、成本、模型、工具、trace），而不是一列 token 行。卡片由智能体
  观察器自己绘制，所以卡片长什么样只有一份实现，而不是两份。可实时配置：

  ```toml
  [ui.sidebar]
  agents_view = "cards"    # 或 "legacy" 使用 herdr 的行列表
  agents_hide_idle = false
  ```

- **键盘智能体导航**——`prefix+a` 把焦点移入 Agents 侧边栏。两个区域：`j`/`k`
  在卡片间移动，`l` 进入选中卡片的工具调用 trace，在那里 `o` 打开详情面板，
  显示状态、耗时、时间戳和完整保留的参数。移动卡片光标刻意*不*改变 pane
  焦点——只有 Enter 才确认——所以浏览一个长列表永远不会让主视图乱跳。鼠标
  全程可用：点一下 trace 行选中，再点一下打开。

- **标签岛**——标签栏是一个胶囊：当前标签是带标题的药丸，其他标签是圆点
  （或数字、或标签名），切换时有弹簧动画。后台智能体完成或被阻塞时会留下一条
  通知记录；胶囊上随即显示铃铛和未读数，`prefix+i`（或点击铃铛）打开面板，
  Enter 跳到该记录的 workspace、tab 和 pane，`r` 全部标为已读，`c` 清空。
  Toast 锚定在胶囊上。所有设置都实时重载；`ui.tab_bar_style = "classic"` 恢复
  herdr 的标签栏。

  ```toml
  [ui.island]
  position = "center"     # 或 "left"
  display = "dots"        # 或 "numbers"、"labels"
  arrivals = "toast"      # 或 "silent"：静默收集记录
  bell = "🔔"             # "!" 为 ASCII；任何一到两格宽的字形都可以
  ```

- **紧凑栏智能体标记**——侧边栏折叠成窄栏时，每一行都带一个两格宽的标记，
  标明该 pane 里运行的是哪个智能体，扫一眼就知道谁在哪。`compact_rail_numbers`
  和 `compact_rail_leading` 控制每行前导显示什么，`[ui.sidebar.compact_rail_marks]`
  可按智能体覆盖标记：

  ```toml
  [ui.sidebar]
  compact_rail_leading = "agent"
  compact_rail_numbers = true          # agent 模式下控制 workspace 行编号
  [ui.sidebar.compact_rail_marks]
  claude = "Cl"                        # 覆盖某个智能体的标记
  ```

- **禁用二进制自更新**——二进制自更新和产品公告都被刻意禁用。本分叉永远不会
  把原版 herdr 装到自己头上。发布构建的正常会话仍默认启用来自 herdr.dev 的
  智能体检测清单更新；如需禁用，请在 `config.toml` 中设置
  `update.manifest_check = false`。

智能体观察器、自动标题、卡片和键盘导航是**仅 Unix**（macOS 和 Linux）的；
紧凑栏智能体标记和标签岛在所有平台上都会构建。除此之外，Windows 构建的是
上游功能集。

## 接下来

- **pane 卡片与工作树流程**——带智能体字形、状态和工作树徽章的卡片式 pane
  头部；以及把"在新工作树里开一个智能体 pane"合并成一个动作，直接拆分当前
  tab 而不是新建 workspace。
- **导航器中的智能体行**——每个 workspace/tab 条目下显示各智能体的状态芯片和
  最新任务消息；点击即聚焦该 pane。
- **本地 hunk 视图**——基于进程内 git 引擎的 TUI diff pane：限定于工作树的
  文件列表、按 hunk 渲染与导航，先做只读。

仍部分推迟：品牌改名覆盖了可执行文件以及配置和状态目录，但**不**包括 `HERDR_*`
环境变量和 `herdr.sock` / `herdr-client.sock` 文件名。它们是集成协议——仅
`herdr-agent-watcher` 一个就读取其中九个变量——改名会让插件失效却没有任何用户
可见的收益。见 [`FORK.md`](FORK.md) 中推迟的品牌改名范围。

## 试一试

没有预构建二进制，没有安装器，没有发布渠道。从源码构建。

**前置条件。** Rust 1.96.1（固定在 `rust-toolchain.toml` 中，`rustup` 会自动
选用）和 **Zig 0.15.2**，用于编译供应的 libghostty-vt。`PATH` 里有*更新*的
Zig 会失败——版本必须精确。Zig 缓存为空时先运行 `scripts/preseed_zig_cache.sh`，
否则构建会在拉取依赖 tarball 时失败。

```bash
git clone https://github.com/winoooops/vimeflow-terminal
cd vimeflow-terminal
scripts/preseed_zig_cache.sh      # 仅在 Zig 缓存为空时需要
cargo build --release
./target/release/vimeflow
```

可执行文件叫 `vimeflow`，不是 `herdr`。想要的话把它放到 `PATH` 里：

```bash
ln -s "$PWD/target/release/vimeflow" ~/.local/bin/vimeflow
```

### 与 herdr 共存

vimeflow 使用自己的目录，所以已安装的 herdr 不受影响：

| | vimeflow | 上游 herdr |
| --- | --- | --- |
| 可执行文件 | `vimeflow` | `herdr` |
| 配置 | `~/.config/vimeflow/` | `~/.config/herdr/` |
| 状态 | `~/.local/state/vimeflow/` | `~/.local/state/herdr/` |

vimeflow 从一份**干净的配置**起步；不会从已安装的 herdr 继承任何东西。想沿用
设置，手动把 `~/.config/herdr/config.toml` 复制过来。

两者可以**同时运行**：服务端会把自己的 socket 路径导出给它启动的 pane，所以
每一边的子进程都会回到启动它的那一边。`HERDR_*` 变量名和 `herdr.sock` 文件名
是刻意共用的，好让插件继续工作，这带来一个后果——*在 herdr 的 pane 里*启动
vimeflow 会继承该 pane 的 `HERDR_SOCKET_PATH`，从而连到 herdr。请在任何会话
之外的终端里启动它，或者清掉这些变量：

```bash
env -u HERDR_ENV -u HERDR_SOCKET_PATH -u HERDR_CLIENT_SOCKET_PATH vimeflow
```

用 `vimeflow status server` 查看当前连的是哪一边——socket 路径会说明归属。

有一样东西确实无法共享：**Claude 指标桥接**是全局 `~/.claude/settings.json`
里的一个 hook，只能由一个安装拥有。最后一个运行过 `watcher claude-bridge enable`
的那一方拿到 Claude 的上下文、缓存和成本数字；另一方这些字段显示为 `—`。

### 配置

```bash
vimeflow --default-config > ~/.config/vimeflow/config.toml
```

每个设置都列出并**注释掉**，显示其默认值。只取消注释你想改的。分叉专属的设置
有标注，并集中在文件末尾。

### 一个值得知道的坑

如果你的卡片全都显示 `— no telemetry`，说明独立的智能体观察器*插件*处于启用
状态，正在和内置的那个打架。插件会在服务端启动时拉起自己的守护进程，取代内嵌
的观察器；内嵌的随即退出，并且按设计不会重启。禁用插件：

```bash
vimeflow plugin disable herdr-agent-watcher
vimeflow server stop && vimeflow      # 重启以生效
```

插件自己的 `open-sidebar` 快捷键无论如何都继续可用——vimeflow 会把它路由到
内置的 Agents 侧边栏。

### 不附加地运行服务端

要启动服务端而不附加 TUI，请运行：

```bash
vimeflow server
```

只有需要停止现有服务端时才运行以下命令；它会终止该会话中的所有 pane：

```bash
vimeflow server stop
```

### 测试

```bash
just test     # 单元测试 + 维护检查
just check    # 完整门禁：lint、测试、Windows 目标 clippy
```

不用 `just` 的话，本分叉 CI 的命令是：

```bash
cargo nextest run --locked -E 'not binary(live_handoff)'   # macOS
cargo nextest run --locked                                  # Linux
```

`live_handoff` 在 macOS 上被排除，与上游一致。会启动真实服务端的集成测试
二进制一次只跑一个（见 `.config/nextest.toml`）；否则它们会争抢文件描述符和
文件系统监听并超时。

## 本分叉如何跟踪 herdr

- `main` 是产品分支，承载所有 Vimeflow 的工作。
- `master` 是 `upstream/master` 的仅快进镜像。那里不提交任何东西。
- 上游发布通过评审分支合并进 `main`，按版本而不是按提交。
- 每个被修改的上游文件都带有文件内的修改声明和 [`FORK.md`](FORK.md) 登记表中
  的一行；无法加注释的文件列在 [`MODIFICATIONS`](MODIFICATIONS) 中。

[`FORK.md`](FORK.md) 是完整记录：分叉基线提交、逐路径的上游修改登记表、合并
流程，以及已知的基线测试行为。[`AGENTS.md`](AGENTS.md) 是给在本仓库工作的 AI
智能体的指南。

## 许可与致谢

本分叉采用与 herdr 相同的 [Apache License 2.0](LICENSE)，并原样保留上游的
LICENSE。

herdr 的版权归 herdr 贡献者所有，由 [@ogulcancelik](https://github.com/ogulcancelik)
创建和维护。本项目的存在完全得益于那份工作是开源的，功劳和支持都应归于上游——
**如果本分叉对你有用，请[赞助 herdr](https://github.com/sponsors/ogulcancelik)**，
而不是本分叉。

供应的依赖各有其许可：`libghostty-vt`（MIT，© Mitchell Hashimoto）和
`portable-pty`（MIT，© Wez Furlong）。供应的 `libghostty-vt/pkg` 目录树还包含
其他条款下的第三方材料；本分叉未来任何二进制分发都必须附带覆盖它们的许可清单。

"herdr" 是上游项目的名称，在此仅用于说明本分叉的来源。本项目不主张对它的任何
权利。
