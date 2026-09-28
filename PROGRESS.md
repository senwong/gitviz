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
