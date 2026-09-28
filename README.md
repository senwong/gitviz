# gitviz

一个**独立**的、支持**多仓库**的 Git Graph 查看器，基于 Zed 的 `gpui` 构建。

它从 Zed 仓库里只取用 UI 框架层（`gpui` / `gpui_platform`），不依赖
`project` / `workspace` / `editor` / `client` 等 Zed 应用层，因此比 Zed 本身小很多，
启动也更快。目标是做成一个轻量、纯粹的 git 图工作台。

## 特性（当前）

- 多仓库：命令行传入若干路径，自动发现其中的 git 仓库
- `⌘P` 命令面板：输入即过滤仓库（按名字/路径），`↑`/`↓` 选择，`Enter` 切换
- 只读提交图：按 lane 着色，显示 `短 SHA  提交主题`
- `⌘R` 重新加载当前仓库，`⌘Q` 退出

## 前置依赖

- macOS
- [rustup](https://rustup.rs)（本仓库 `rust-toolchain.toml` 固定 `1.98.1`）
- 网络（首次构建时）。`gpui` 尚未作为通用 SDK 发布到 crates.io，本仓库通过
  **git 依赖**固定到 Zed 仓库的一个 commit 来引用它，因此**不依赖任何本地 checkout**（无需 `../zed`）。

> 构建会编译 `gpui` 及其依赖树，**比较消耗 CPU 和内存**。
> 建议放在非工作时段跑。

## 构建与运行

`script/build` 是对 `cargo` 的封装：使用 rustup 的工具链，并默认
`CARGO_INCREMENTAL=0`，避免 `target/` 体积膨胀到几十 GB。

```sh
cd ~/projects/gitviz

# 首次编译检查（先跑这个，方便定位问题）
./script/build check

# 运行：传入一个或多个仓库 / 包含仓库的目录
./script/build run -- ~/projects/jp-cms ~/projects/umu_node
```

不带参数时，默认扫描当前工作目录。

```sh
# release 构建（更小更快，编译更久）
./script/build build --release
```

## 打包 macOS .app / DMG

```sh
# 构建 release，组装 gitviz.app，ad-hoc 签名，并生成 gitviz.dmg
./script/package

# 跳过编译，只打包 target/release/gitviz
./script/package -s

# 自定义名字 / 输出目录
./script/package -n GitViz -o ~/projects
```

产物：`gitviz.app` 和 `gitviz.dmg`（默认放仓库根目录）。`.app` 用 ad-hoc 签名，
首次打开如被 Gatekeeper 拦截，可在「系统设置 → 隐私与安全性」里允许，或右键 → 打开。

## 快捷键

| 快捷键 | 作用 |
| --- | --- |
| `⌘P` | 打开/关闭命令面板 |
| `⇧⌘P` | 打开/关闭仓库切换面板 |
| `⌘F` | 打开/关闭查找 |
| `⌘G` / `⇧⌘G` | 跳到下一个 / 上一个匹配 |
| `⌘R` | 重新加载当前仓库（`git log`） |
| `⌘H` | 滚动到 HEAD |
| `⌘S` / `⇧⌘S` | 跳到下一个 / 上一个 stash |
| `⌘T` | 循环切换主题 |
| `⌘↑` / `⌘↓` | 跳到同分支的父 / 子提交（按住 `⇧` 走替代分支） |
| `↑` / `↓` | 上下选择提交 |
| `⌘`/`Ctrl` + 点击 | 进入提交对比（选中 Uncommitted 时为工作区↔提交对比） |
| `Enter` / `Esc` | 提交对话框 / 关闭面板、菜单、diff |

原生菜单栏还提供 **File / View / Window / Help**：File → Open Repository…、Open Workspace…、Save Workspace…、Open Recent…、Switch Repository…。

## 目录结构

```
src/main.rs        gpui 应用启动、解析命令行路径、开窗口
src/discovery.rs   从给定路径发现 git 仓库（root 及两层子目录，按规范路径去重）
src/git.rs         通过 shell 调 git（`git log` / `rev-parse`），不依赖 libgit2
src/layout.rs      为提交分配 lane（首父同列，merge 父另起列）
src/view.rs        主视图渲染、命令面板、键盘处理
script/build       cargo 包装脚本
```

## 状态与已知限制

- **已编译通过，测试通过（87 项）**：`./script/build check --all-targets` 无错误/警告，`./script/build test` = 64 单测 + 23 集成全部通过，`./script/build build` 产出可启动的 `target/debug/gitviz`。
- 已实现：lane 连线（rounded / angular）、提交详情与 diff、写操作（checkout / cherry-pick /
  merge / rebase / reset / revert / drop / push / fetch / pull / tag / stash）、
  过滤器与搜索、深浅主题、代码审查、多仓库等（见下方“功能对齐”清单）。
- 暂未实现：网络头像（需要给 `Application` 配置 `AssetSource` 与 http client）。

## 路线图

1. ~~编译跑通~~（已完成）
2. 按下方“功能对齐 mhutchie/vscode-git-graph”清单继续补齐可选细节（网络头像等）

## 功能对齐 mhutchie/vscode-git-graph

目标：覆盖该扩展的全部功能。当前进度（`[x]` 已有 / `[~]` 部分 / `[ ]` 待做）：

- 图显示
  - [x] 本地分支 / 远程分支 / 标签（`%D` 引用标签）；引用以**彩色 pill** 呈现（HEAD=实心蓝、本地分支=蓝、远程=灰、tag=绿）
  - [x] 未提交变更节点（Uncommitted Changes）
  - [x] Stash 节点
  - [x] 初始加载 + 加载更多（上限定为 2000）
  - [x] 远端 HEAD 符号引用、仅被标签引用的提交、reflog 提交（开关）
- 提交操作（当前全部通过右键菜单）
  - [x] Cherry Pick / Revert / Merge / Rebase / Reset(soft/mixed/hard) / Checkout
  - [x] Create Branch / Create Tag Here
  - [x] Push（当前分支）/ Pull（当前分支）
  - [x] 分支操作（checkout/create/rename/delete/merge/rebase/pull/push）
  - [x] 标签操作（create / create annotated / push / delete UI）
  - [x] 提交 drop、annotated tag 详情（tagger/date/message）
- 提交详情
  - [x] 提交信息 / 作者 / 邮件 / 改动文件（+/-）
  - [x] 点击文件查看 diff（内置文本 diff 覆盖层）
  - [x] 复制 SHA / 复制提交信息
  - [x] 打开文件当前版本（Open）/ 复制路径（Copy）/ 按提交打开（Rev）/ 正文 URL
  - [x] 签名状态、mailmap（开关）
- 提交对比
  - [x] Cmd/Ctrl 点击第二个提交进入对比，列出差异文件
  - [x] 选中 Uncommitted Changes 后 Cmd/Ctrl 点击提交 → 工作区与该提交对比
  - [x] 对比视图打开文件 diff
- 代码审查（Code Review）
  - [x] 已审查文件标记（`[x]`/`[ ]`）；进行中的审查中，未看文件加粗，查看 diff / 打开文件后自动去掉加粗
  - [x] 持久化 + 90 天过期；工作区级命令（End all / 停止当前提交）
- 未提交变更
  - [x] 显示与选择、查看文件列表
  - [x] Stash 操作（push/apply/pop/drop/branch）+ 未提交变更的 Stash/Discard（整仓 + 单文件）
  - [x] Stash 详情：文件列表 + 每个文件的 diff（`git stash show`）
- 悬浮提示
  - [x] 是否属于 HEAD 祖先（颜色 + 底部信息条 “in HEAD / not in HEAD”）
  - [x] 详情显示包含该提交的分支/标签；悬浮信息条显示 refs / HEAD，并懒加载并缓存 “contained in: 分支/标签/stash”
- 分支过滤
  - [x] 过滤面板（点击选择分支，客户端过滤）；显示相对 HEAD 的 ↑ahead / ↓behind
  - [x] `Show All` 已做；custom glob patterns（`glob_match` + 设置/命令面板 “Add branch glob…” + `.gitviz.conf`）
- 查找（Find）
  - [x] 文案搜索（提交信息含正文/作者/哈希/日期/ref 名），Cmd+F
  - [x] 高亮匹配（⌘G 导航）、date/ref 名称匹配
- 仓库设置
  - [x] remotes 查看/增删改/fetch/prune
  - [x] Issue Linking、Pull Request Creation（GitHub/GitLab/Bitbucket + `.gitviz.conf` 自定义 `pr_provider` 模板）
  - [x] 配置导出到仓库文件（.gitviz.conf）
- 键盘快捷键
  - [x] Cmd+F / Cmd+R / Cmd+S(shift) / Cmd+H / Up/Down / Enter / Esc
  - [x] ⌘↑/⌘↓ 同分支父子跳转；⇧ 变体（沿替代分支）
- 列与外观
  - [x] Date / Author / Commit 列显示开关（设置面板）；短日期 / 相对日期（“3 days ago”）
  - [x] 深浅主题切换；**10 个内置主题**（5 深 + 5 浅：Dark+、One Dark Pro、Dracula、Nord、Solarized Dark、GitHub Light、One Light、Solarized Light、Quiet Light、Gruvbox Light）；命令面板 / 菜单 View → Select Theme… 选择，`⌘T` 循环切换，选择会持久化
  - [x] 列宽 +/-（含拖拽）、车道配色预设（自定义颜色 + rounded/angular graph style）；reference label 对齐/合并已做；完整 ref 名开关
- 消息渲染
  - [x] Emoji shortcode / gitmoji 替换
  - [x] Markdown 子集（粗体/斜体/行内代码）
- 其他
  - [x] 多仓库（Cmd+P 搜索切换 + 仓库排序 + 发现深度 + 动态 Add/Remove repository）
  - [x] 未打开仓库时显示欢迎页（Open Repository / Open Workspace 按钮 + Open Recent 列表）
  - [x] 打开仓库：原生目录选择器 “Open repository…” / 把文件夹或仓库拖拽到窗口
  - [x] Open Recent：记住最近打开的仓库/工作区（`~/.config/gitviz/recent`）
  - [x] 原生菜单栏 File / View / Window / Help（Open Repository/Workspace、Save Workspace、Open Recent、Switch Repository、Minimize/Zoom、Quit）
  - [x] Workspace 文件（`.gitviz-workspace`）：一行一个仓库/目录，支持 `#` 注释与 `~`；命令面板 “Open workspace…” / “Save workspace…”，也可作为命令行参数传入
  - [~] 首字母头像（网络头像省略）；命令面板（⇧⌘P）已做；状态栏入口 N/A
  - [x] 仓库最大发现深度可配置

## 与 Zed 的关系

本仓库**只依赖 Zed 的 UI 框架层**，且通过 git 依赖固定到一个 commit：

```toml
gpui = { git = "https://github.com/zed-industries/zed", rev = "a8535d86b7c4f8061c35b1802734be33201d815b", default-features = false, features = ["font-kit", "stacker"] }
gpui_platform = { git = "https://github.com/zed-industries/zed", rev = "a8535d86b7c4f8061c35b1802734be33201d815b", default-features = false, features = ["font-kit"] }
```

- **不依赖任何本地仓库**（无需 `../zed`）。
- 首次构建时 cargo 会克隆 Zed 仓库到 `~/.cargo/git`（仅构建 `gpui` 及其依赖）。
- 想跟随自己的 Zed fork 时，把 `git`/`rev` 换成你的 fork 与其 commit 即可。
