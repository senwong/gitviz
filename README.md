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
- 本地存在 Zed 源码 checkout，路径为本仓库的 `../zed`
  （即 `/Users/<you>/projects/zed`）。`gpui` 尚未作为通用 SDK 发布，
  这里通过 path 依赖直接引用它。

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

## 快捷键

| 快捷键 | 作用 |
| --- | --- |
| `⌘P` | 打开/关闭仓库命令面板 |
| `↑` / `↓` | 在面板中移动选择 |
| `Enter` | 切换到选中的仓库 |
| `Esc` | 关闭面板 |
| `⌘R` | 重新加载当前仓库的提交 |
| `⌘Q` | 退出 |

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

- **这是第一版脚手架，尚未编译验证。** 首次 `cargo check` 可能需要修正若干
  `gpui` API 细节（元素方法名、`KeyDownEvent` 字段、`cx.quit()` 等）。
- 目前只画每一行的着色圆点，**还没有 lane 之间的连线**。
- 没有提交详情 / diff 面板。
- 没有写操作（checkout、cherry-pick、merge、rebase、push 等）。
- 没有主题切换（目前是固定深色配色）。

## 路线图

1. 编译跑通并修掉第一版的问题
2. lane 连线（canvas 绘制父子连线与 merge 曲线）
3. 提交详情面板（提交信息、改动文件、diff）
4. 右键操作（cherry-pick / revert / merge / rebase / checkout / push）
5. 过滤器（local/remote/tags/first-parent）与搜索
6. 主题与浅色模式

## 与 Zed 的关系

本仓库只依赖 Zed 的 UI 层：

```toml
gpui = { path = "../zed/crates/gpui", default-features = false, features = ["font-kit", "stacker"] }
gpui_platform = { path = "../zed/crates/gpui_platform", default-features = false, features = ["font-kit"] }
```

因此需要保证 `../zed` 存在且与其 `rust-toolchain.toml`（1.98.1）一致。
