# 进度与接棒说明（Progress / Handoff）

> 这个文档用于随时接棒：读完它就能知道**目标、当前状态、下一步、以及注意事项**。
> 每次完成一批工作后请更新“当前状态”“待办”“编译问题清单”。

## 目标

`gitviz` = 一个**独立**的、**多仓库**的 Git Graph 查看器，基于 Zed 的 `gpui`（不是 Zed 的一部分）。
最终目标：**对齐 `mhutchie/vscode-git-graph` 的全部功能**（功能清单见 README 的“功能对齐”一节）。

## 仓库与依赖

- 本地：`~/projects/gitviz`
- 远端：`https://github.com/senwong/gitviz`（默认分支 `main`，`origin`）
- 依赖：`gpui` / `gpui_platform` 通过 **path** 引用本地 Zed：`../zed/crates/...`
  （`gpui` 未作为通用 SDK 发布，必须依赖本地 Zed checkout）
- 工具链：`rust-toolchain.toml` 固定 `1.98.1`（与 Zed 一致）
- 构建脚本：`script/build`（用 rustup 工具链，默认 `CARGO_INCREMENTAL=0`）

## 重要约定

1. **工作时间不 build**（gpui 依赖树编译很吃 CPU/内存）。
2. **开发阶段先写代码，攒一批后集中 build 修错**。
3. 提交前不用 build；但要在本文档记录“编译问题清单”的进展。

## 当前状态（务必先看）

- **代码已写约 3200+ 行，但从未编译过。** 第一次 `cargo check` 一定需要修一批错误。
- 最新提交：
  ```
  1fc6e36 README: tick off completed feature-parity items
  dec1ade Add emoji/markdown, remotes management, PR/issue links, load more
  253a0eb Add refs, uncommitted/stash nodes, columns, compare, detail diff and more
  fc1001e Rename project to gitviz
  b2cdd44 Initial scaffold: standalone multi-repo git graph viewer on gpui
  ```

### 已写但未编译的批次

- 批次 1：脚手架、多仓库、Cmd+P、git log + lane 布局
- 批次 2：refs、未提交/Stash 节点、多列、对比、详情+diff、代码审查标记、分支过滤、Find、设置、主题
- 批次 3：emoji、markdown、remotes 管理、PR/issue 链接、load more、签名、mailmap/reflog、代码审查持久化（90 天）、仓库配置导入导出（`.gitviz.conf`）、仓库排序、打开文件/复制路径、消息 URL、悬浮信息条、Drop commit
- 批次 4：改动文件状态字母（A/M/D/R/U）、详情文件树（compact folders）、作者头像（首字母圆形）、车道配色预设、remote HEAD refs、annotated tag 详情、列宽字段；**并补上单元测试 + 集成测试 + lib target**
- 批次 5：命令面板（⇧⌘P）、查找导航（⌘G / ⇧⌘G）+ 匹配高亮、参考标签对齐（combine + align）、仅标签提交（only-tags）、两提交间文件 diff、签名验证详情；**新增 `log_ref_args`/`compare_file_diff`/`signature_details` 及对应单测/集成测试**
- 批次 6：列宽 +/- 调节、仓库发现深度可调（`discover_with_depth`）、详情显示“包含该提交的分支/标签”；**新增 `adjust_width` 单测、`discovery` 深度单测、`branches_containing`/`tags_containing` 集成测试**

## 文件结构

```
Cargo.toml          gpui / gpui_platform path 依赖；edition 2024
rust-toolchain.toml 1.98.1
script/build        cargo 包装（rustup + 关闭 incremental）
src/main.rs         启动 gpui、解析路径、开窗口
src/discovery.rs    发现仓库（root + 两层子目录）
src/git.rs          shell 调 git：log/status/stash/refs/diff/remotes/URL/签名 + 全部写操作
src/layout.rs       分配 lane，并计算每行的连线（through/incoming/outgoing/top/bottom）
src/emoji.rs        :shortcode: / gitmoji 替换
src/markdown.rs     内联 Markdown（粗体/斜体/行内代码）→ span
src/theme.rs        深/浅主题配色
src/view.rs         主视图：图、列、refs、未提交/stash 节点、详情、对比、右键菜单、
                    命令面板、分支过滤、设置、Find、diff 覆盖层
README.md           使用说明 + “功能对齐 vscode-git-graph”清单
PROGRESS.md         本文件
```

## 下一步（建议顺序）

1. **集中 build 修错（最高优先级）**
   ```sh
   cd ~/projects/gitviz
   ./script/build check
   ```
   反复修到通过；再 `./script/build run -- ~/projects/jp-cms ~/projects/umu_node` 手动验证。
2. 修完后按 README 清单继续补功能（见下）。

## 编译时预计要修的点（build pass checklist）

- [ ] `uniform_list` 闭包签名（`Fn(Range, &mut Window, &mut App)`）与 `'static` 捕获
- [ ] `gpui::canvas(prepaint, paint)` 的 prepaint/paint 形参与返回值
- [ ] `PathBuilder::stroke/move_to/line_to/curve_to/build` 与 `Window::paint_path`
- [ ] `.hover(...)` 的接收者类型（`StyleRefinement` vs 元素）
- [ ] 颜色类型：`Theme` 用的是 `gpui::Rgba`（`rgb()`），与 `.text_color()/.bg()` 的 `Into` 是否匹配
- [ ] `Min/Max` 等 sizing 简写是否存在：`min_w_0 / min_h_0 / h / w / px`
- [ ] `ElementId`：`("id", usize)`、`String`（`impl Into<ElementId>`）
- [ ] `WeakEntity::update(cx, ...)` 在 `&mut App` 下的用法
- [ ] `App::quit()`、`MouseDownEvent.modifiers.secondary()`（已确认存在，注意用法）
- [ ] `span_element` 内局部变量命名与 trait 名冲突（改名为 `node`）
- [ ] 借用/生命周期错误（`self` 不可变借用 + `self.xxx = ...` 赋值处）
- [ ] `git.rs` 里 `line.get(3..)`、`--date=iso` 输出解析
- [ ] `let ... && let ...`（let-chains）在 edition 2024 下应可用，若报错改成嵌套 `if`

## 待办 backlog（按 README 清单）

- [ ] 列宽拖拽、graph style / 自定义颜色、reference label 对齐/合并细节
- [ ] 代码审查的**持久化**（当前仅内存）+ 90 天自动过期 + 工作区级命令
- [ ] 打开文件当前版本、复制文件路径、提交正文 URL 可点击
- [ ] reflog / remote-head / tag-only 提交的入口开关（git 层已部分支持）
- [ ] 头像抓取（可选）
- [ ] 仓库下拉顺序（Cmd+P 已有）
- [ ] 配置导出到仓库文件
- [ ] 状态栏入口 / 命令面板命令（standalone 环境下等价物）
- [ ] 提交 drop、annotated tag 详情、签名验证细节
- [ ] 悬浮 tooltip：包含该提交的分支/标签/stash

## 测试

- **单元测试**（`#[cfg(test)] mod tests`）：
  - `emoji.rs`：shortcode 替换
  - `markdown.rs`：粗体/斜体/行内代码解析
  - `layout.rs`：lane 分配（线性 / merge / 不变量）
  - `config.rs`：`.gitviz.conf` 读写往返
  - `git.rs`：`parse_remote`（GitHub/GitLab/Bitbucket、未知 host）、`urlencode`
- **集成测试**（`tests/git_integration.rs`）：在临时目录 `git init` 真实仓库，跑 log/status/branches/tag/stash/commit_detail/remotes。
- 为支持集成测试，新增了 `src/lib.rs`（lib target），`main.rs` 改为引用 `gitviz::...`。
- 运行（编译通过后）：
  ```sh
  ./script/build test
  ./script/build test --test git_integration
  ```
- **注意**：这些测试目前也**还没跑过**（代码未编译）。第一次 build 后用它们验证。

## 快速上手（给接棒的自己）

```sh
cd ~/projects/gitviz
git pull

# 只读功能验证（非工作时段）
./script/build check
./script/build run -- ~/projects/jp-cms ~/projects/umu_node ~/projects/umu_service_pages

# 快捷键：⌘P 切仓库，⌘F 查找，⌘R 刷新，⌘T 主题，⌘S 下一个 stash，↑↓ 选择，⌘/Ctrl+点击对比
```
