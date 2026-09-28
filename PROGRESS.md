# 进度与接棒说明（Progress / Handoff）

> 这个文档用于随时接棒：读完它就能知道**目标、当前状态、下一步、以及注意事项**。
> 每次完成一批工作后请更新“当前状态”“待办”“编译问题清单”。

## 目标

`gitviz` = 一个**独立**的、**多仓库**的 Git Graph 查看器，基于 Zed 的 `gpui`（不是 Zed 的一部分）。
最终目标：**对齐 `mhutchie/vscode-git-graph` 的全部功能**（功能清单见 README 的“功能对齐”一节）。

## 仓库与依赖

- 本地：`~/projects/gitviz`
- 远端：`https://github.com/senwong/gitviz`（默认分支 `main`，`origin`）
- 依赖：`gpui` / `gpui_platform` 通过 **git 依赖**固定到 Zed 仓库的 commit（上游 `zed-industries/zed`），**不依赖任何本地仓库**，也无需 `../zed`
  （`gpui` 未作为通用 SDK 发布，必须依赖本地 Zed checkout）
- 工具链：`rust-toolchain.toml` 固定 `1.98.1`（与 Zed 一致）
- 构建脚本：`script/build`（用 rustup 工具链，默认 `CARGO_INCREMENTAL=0`）

## 重要约定

1. **工作时间不 build**（gpui 依赖树编译很吃 CPU/内存）。
2. **开发阶段先写代码，攒一批后集中 build 修错**。
3. 提交前不用 build；但要在本文档记录“编译问题清单”的进展。

## 当前状态（务必先看）

- **已编译通过并全部测试通过（2026-09-28）**：`./script/build check --all-targets` 无错误/警告；`./script/build test` = **64 单测 + 23 集成（87）全部通过**；`./script/build build` 产出 `target/debug/gitviz` 并能启动。
- 非 UI 逻辑也可用 `./script/pure-test` 快速验证。
- 最新提交：
  ```
  de314d6 Fix remaining compile errors so the UI builds cleanly
  5e882d4 Fix use-after-move of weak handle in render header
  396f6b8 Shrink dev build artifacts (no debug info, no incremental)
  58b1551 Commit Cargo.lock (dependencies fetched)
  2569469 Make script/build executable
  ```

### 已写但未编译的批次

- 批次 1：脚手架、多仓库、Cmd+P、git log + lane 布局
- 批次 2：refs、未提交/Stash 节点、多列、对比、详情+diff、代码审查标记、分支过滤、Find、设置、主题
- 批次 3：emoji、markdown、remotes 管理、PR/issue 链接、load more、签名、mailmap/reflog、代码审查持久化（90 天）、仓库配置导入导出（`.gitviz.conf`）、仓库排序、打开文件/复制路径、消息 URL、悬浮信息条、Drop commit
- 批次 4：改动文件状态字母（A/M/D/R/U）、详情文件树（compact folders）、作者头像（首字母圆形）、车道配色预设、remote HEAD refs、annotated tag 详情、列宽字段；**并补上单元测试 + 集成测试 + lib target**
- 批次 5：命令面板（⇧⌘P）、查找导航（⌘G / ⇧⌘G）+ 匹配高亮、参考标签对齐（combine + align）、仅标签提交（only-tags）、两提交间文件 diff、签名验证详情；**新增 `log_ref_args`/`compare_file_diff`/`signature_details` 及对应单测/集成测试**
- 批次 6：列宽 +/- 调节、仓库发现深度可调（`discover_with_depth`）、详情显示“包含该提交的分支/标签”；**新增 `adjust_width` 单测、`discovery` 深度单测、`branches_containing`/`tags_containing` 集成测试**
- 批次 7：查找匹配扩展（日期/ref 名）、stash 建分支（菜单+提示框）、tag 的 Push/Delete 按钮、按提交打开文件（Rev）；**`find_matches` 单测更新、`show_file`/`delete_tag`/`stash_branch` 集成测试**
- 批次 8：merge 变体（no-ff / squash）、cherry-pick allow-empty、fetch prune / prune-tags 开关、force push 标记；**`fetch_args`/`push_force_flag` 单测、cherry-pick/merge 集成测试**
- 批次 9：接近底部自动加载更多（`should_load_more` 单测）、README 清单核对
- 批次 10：同分支父子跳转（⌘↑/⌘↓，`find_parent_index`/`find_child_index` 单测）、未提交变更菜单 Stash/Discard（`discard_all` 集成测试）
- 批次 11：分支过滤面板的 Checkout/Rename/Delete 与 Show All、glob 匹配纯函数（`glob_match` 单测）
- 批次 12（A 项）：`.gitviz.conf` 支持 `branch_globs` 与自定义 `lane_colors`；分支按 glob 过滤、车道颜色自定义；短日期格式；命令面板“Resume last code review”；清空 globs。**新增 `parse_hex_color`/`filter_by_globs`/`format_date`/`ReviewStore::latest_commit` 单测、config 往返测试扩展**
- 依赖调整：`gpui`/`gpui_platform` 改为 **git 依赖（固定上游 rev）**，gitviz 不再依赖任何本地仓库
- 批次 13（A 项）：右键菜单可见性（`.gitviz.conf` 的 `hidden_actions`）、⌘⇧↑/⌘⇧↓ 替代分支导航、Gravatar 头像链接（点击打开）；新增 `md5` 依赖。**新增 `visible_actions`/`find_alt_parent_index`/`find_alt_child_index`/`gravatar_url` 单测、config 往返扩展**
- 批次 14（A 项）：**列宽拖拽**（表头分隔条 + `on_mouse_move`/`on_mouse_up`）、**On Load 滚动到 HEAD**（`find_head_commit_index` 单测）
- 批次 15（A 项）：**滚动到底自动加载更多**（`UniformListScrollHandle` + `near_bottom` 单测）
- 批次 16（A 项）：**自定义 emoji 映射**（`.gitviz.conf` 的 `emoji_mappings=code:emoji`，`emoji::replace_with` 先自定义后内置）、**停止当前提交的代码审查**（命令面板 `end-current-review` + `ReviewStore::remove_commit`）。**新增 `replace_with`/`remove_commit` 单测、config 往返扩展**
- 批次 17（A 项）：**graph style（rounded / angular）** 与 **分支 glob 的 UI 入口**。`layout::GraphStyle` + `layout::row_segments`（把每行连线抽成可测的纯函数 `Segment`/`SegmentKind`），绘制改用该纯函数；`.gitviz.conf` 新增 `graph_style`；设置面板新增 “Angular graph connectors”；命令面板/设置新增 “Add branch glob…”（提示输入后写入配置并重载）。**新增 `row_segments`/`GraphStyle::parse`/`parse_emoji_mappings` 单测、config 往返扩展**
- 批次 18（A 项）：**remote URL 编辑**（`git remote set-url`，设置面板 “Edit URL” → 提示框预填当前 URL）与 **Fetch into Local Branch**（`git fetch <remote> <rb>:<lb>`，命令面板/设置 “Fetch into local branch…”）。**新增 `fetch_into_args` 单测、`set_remote_url`/`fetch_into_branch` 集成测试**
- 批次 19（A 项）：**代码审查的加粗/去粗**（未看文件加粗，打开 diff / Open / Rev 后 `ReviewStore::mark` 自动标记并去粗，`has_commit` 判定审查是否进行中）；**复制 ref 名**（stash 菜单 “Copy Stash Reference”、分支面板 “Copy”）。**新增 `ReviewStore::mark`/`has_commit` 单测**
- 批次 20（A 项）：**悬浮包含信息**——hover 提交时懒计算并缓存 `git branch --contains` / `git tag --contains`（`Containment`），底部信息条追加 “contained in: …”；切换/刷新仓库时清缓存
- 批次 21（A 项）：**分支面板新增 Merge / Rebase**（`git merge <branch>` / `git rebase <branch>`，沿用已有 git 实现）。**新增 `pulls_from_a_remote` 集成测试**（覆盖此前未测的 `git::pull`）
- 批次 22（A 项）：**创建 annotated tag（含 message）**（`git tag -a <name> <sha> -m <message>`；提交右键 “Create Annotated Tag Here…”，提示框输入 `name message`）。**新增 `annotated_tag_args` 单测、`creates_annotated_tag_with_message` 集成测试**
- 批次 23（A 项）：**工作区与提交对比**——选中 Uncommitted 后 Cmd/Ctrl 点击提交进入该提交↔工作区对比（`git diff <sha>` / `git diff <sha> -- <path>`）；顺带修复 compare 模式下 `detail` 为空导致对比面板不渲染的问题（现在始终加载 detail 并额外填充 compare_files）。**新增 `diffs_working_tree_against_a_commit` 集成测试**
- 批次 24（A 项）：**动态增删仓库**——命令面板/设置 “Add repository…”（支持 `~/` 展开，`expand_tilde`）与 “Remove current repository”（从视图与 roots 中移除）。**新增 `expand_tilde` 单测与命令清单测试**
- 批次 25（A 项）：**Stash 详情**——选中 stash 时用 `git stash show --numstat` 列出文件、`git stash diff`（`stash@{n}^1..stash@{n}`）查看单文件 diff；顺带修正 `ChangedFile` 缺少 `status` 的构造（`select_row` 未提交文件、numstat 解析）并补 `status_entry_letter`。**新增 `lists_and_diffs_stash_contents` 集成测试、`status_entry_letter` 单测**
- 批次 26（A 项）：**相对日期**（“3 days ago”，设置面板 “Relative dates”，`relative_time` 纯函数）。**新增 `relative_time` 分档单测**
- 批次 27（A 项）：**悬浮包含信息补充 stash**——`git::stashes_containing`（`merge-base --is-ancestor` 遍历 stash），`Containment` 增加 `stashes` 并显示在底部信息条。**新增 `stashes_containing_reports_ancestor_commits` 集成测试**
- 批次 28（A 项）：**自定义 PR Provider 模板**——`.gitviz.conf` 的 `pr_provider`（占位符 `{host}/{owner}/{repo}/{base}/{head}`），非空时优先于内置 provider。**新增 `render_pr_template` 单测、config 往返扩展**
- 批次 29（A 项）：**提交正文（body）**——`Commit` 增加 `body`，`git log` 改用记录分隔符解析多行 body；Find 现在也搜索正文。**新增 `reads_commit_body` 集成测试、find 测试扩展**
- 说明：既然 `git log` 的解析从“逐行”改为“按记录分隔符”，需在 build 时重点验证 log 解析（见“编译时预计要修的点”）
- 批次 30（A 项）：**完整 ref 名开关**——`LogFilter::full_refs` + `decorate_args`（`--decorate=full`），设置面板 “Show full ref names”。**新增 `decorate_args` 单测**
- 批次 31（A 项）：**单文件 Discard**——未提交文件行新增 “Discard” 按钮（`git checkout -- <path>` / 未跟踪用 `git clean -f`）。**新增 `discards_a_single_file` 集成测试**
- 批次 32（A 项）：**Pull 当前分支入口**——命令面板/设置新增 “Pull current branch”（接线此前未暴露的 `git::pull`）。**已有 `pulls_from_a_remote` 集成测试覆盖**
- 静态审查修正（对照 pinned gpui rev `a8535d86` 源码）：**`h_flex`/`v_flex` 不在 gpui（在 zed 的 `ui` crate）**，改为在 `view.rs` 内定义本地 `h_flex`/`v_flex`；**`overflow_y_scroll` 只在 `StatefulInteractiveElement`（需要先 `.id(...)`）**，给所有可滚动列表加唯一 `id`。其余 gpui 方法（`when`/`when_some`、`on_hover`/`on_mouse_move`/`on_mouse_up`、`canvas`、`PathBuilder::curve_to(to, ctrl)`、`write_to_clipboard`、`font_weight`、tailwind 风格方法等）已逐一核对存在。
- 批次 33（A 项）：**自定义 Issue URL 模板**——`.gitviz.conf` 的 `issue_provider`（占位符 `{host}/{owner}/{repo}/{issue}`）；详情面板的 “Create PR”/“Open Issue” 也改为优先使用自定义 provider（此前只用了内置）。**新增 `render_issue_template` 单测、config 往返扩展，并补 `find_urls`/`find_issues`/`combine_refs` 单测**
- 批次 34（A 项）：**分支 ahead/behind**、**文件树单测**
- 批次 35（A 项）：**打开仓库 / 打开 workspace**：
  - 原生目录选择器 “Open repository…”（`App::prompt_for_paths` + `cx.spawn`）
  - 拖拽文件夹/仓库到窗口（`ExternalPaths` + `.on_drop`）
  - Workspace 文件 `.gitviz-workspace`（新 `src/workspace.rs`：`parse`/`serialize`/`expand_tilde`/`expand_roots`），命令面板 “Open workspace…”（原生文件选择器）/“Save workspace…”（`prompt_for_new_path`），也支持作为命令行参数
  - **新增 `workspace` 模块 4 个单测（解析/序列化/tilde/expand_roots）**；`tilde` 逻辑从 `view.rs` 移到 `workspace.rs`
- 验证更新：独立 crate **44 单测 + 23 集成**；全量 **68 单测 + 23 集成 = 91 项全部通过**。
- 验证：非 UI 逻辑（含上述所有 git 单测/集成测试）已在独立 crate 跑通 **36 单测 + 20 集成**；仅 `view.rs`/`theme.rs` 未编译。
- 批次 34（A 项）：**分支 ahead/behind**——`git rev-list --left-right --count <branch>...HEAD`（`parse_ahead_behind`/`ahead_behind`），在 load 时缓存到 `branch_tracking`，分支面板显示 ↑ahead / ↓behind。**新增 `parse_ahead_behind` 单测、`reports_ahead_behind_counts` 集成测试、`build_tree_rows` 文件树单测**
- 验证更新：独立 crate 目前 **37 单测 + 21 集成 = 58 项全部通过**。

## 文件结构

```
Cargo.toml          gpui / gpui_platform git 依赖（固定 rev）；edition 2024
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

1. **已编译通过（2026-09-28）**：`./script/build check --all-targets` 无错误；`./script/build test` 全部通过（**64 单测 + 23 集成 = 87**）；`./script/build build` 产出 `target/debug/gitviz`（36M）并能启动。
2. 继续按 README 清单补功能 / 打磨 UI（见下）。

## 编译情况（build pass，已完成）

`view.rs` 第一次编译共修 12 处错误（脚本已跑通）：

- [x] `ResizeColumn` 重复 `#[derive(Clone, Copy)]`
- [x] `LogFilter` 初始化缺 `full_refs` 字段
- [x] `ElementId`：`("command", &str)` / `("menu", String)` 不能靠 `Into` 自动转换 → 改成 `id` 或 `format!` 生成的 `String`
- [x] `status_letter(file.status, theme)` 需要 `&theme`
- [x] `action_button` 的 `Fn` 闭包里对 `name_fetch`/`name_prune`/`name_remove` 的 `move` 捕获 → 在闭包内 `clone()`
- [x] 文件行审查闭包借用 `file.path` 逃逸 → 预先 `clone()` 成 `review_path`
- [x] 渲染 header 时最后一个 `chip(...)` 把 `weak` move 掉，后面又用 → 改为 `weak.clone()`
- [x] `h_flex`/`v_flex` 不在 gpui（在 zed 的 `ui` crate）→ `view.rs` 内自定义
- [x] `overflow_y_scroll` 属于 `StatefulInteractiveElement` → 可滚动列表都加了唯一 `id`
- [x] `main.rs` 调用 `cx.new` 需要 `use gpui::AppContext as _;`

## 待办 backlog

功能已基本对齐 vscode-git-graph（见 README 清单）。剩余可选：

- [ ] 网络头像（需要给 `Application` 配置 `AssetSource` + http client，工作量较大）
- [ ] 自定义 Pull Request provider 的图形化配置（当前走 `.gitviz.conf`）
- [ ] 更多键盘快捷键可配置化

## 测试

- **单元测试**（`#[cfg(test)] mod tests`）：
  - `emoji.rs`：shortcode 替换
  - `markdown.rs`：粗体/斜体/行内代码解析
  - `layout.rs`：lane 分配（线性 / merge / 不变量）
  - `config.rs`：`.gitviz.conf` 读写往返
  - `git.rs`：`parse_remote`（GitHub/GitLab/Bitbucket、未知 host）、`urlencode`
- **集成测试**（`tests/git_integration.rs`）：在临时目录 `git init` 真实仓库，跑 log/status/branches/tag/stash/commit_detail/remotes。
- 为支持集成测试，新增了 `src/lib.rs`（lib target），`main.rs` 改为引用 `gitviz::...`。
- **快速验证（不需要 gpui，很快）**：`./script/pure-test` 会把纯逻辑模块（config/discovery/emoji/git/layout/markdown/review）复制进一个临时 crate 并跑测试。
- 全量（含 UI）：
  ```sh
  ./script/build test                    # 64 单测 + 23 集成，全部通过
  ./script/build check --all-targets     # 无错误/警告
  ./script/build build                   # 产出 target/debug/gitviz
  ./script/build run -- ~/projects/a ~/projects/b
  ```
- **编译状态**：`view.rs`/`theme.rs` 已编译通过（2026-09-28），二进制可启动。

## 快速上手（给接棒的自己）

```sh
cd ~/projects/gitviz
git pull

# 只读功能验证（非工作时段）
./script/build check
./script/build run -- ~/projects/jp-cms ~/projects/umu_node ~/projects/umu_service_pages

# 快捷键：⌘P 切仓库，⌘F 查找，⌘R 刷新，⌘T 主题，⌘S 下一个 stash，↑↓ 选择，⌘/Ctrl+点击对比
```
