# 本地 fork 维护（手动、可回退）

这些脚本只用于这份克隆，不是发行版安装器。依赖 Bash、Git、GNU coreutils；构建另需已安装的仓库 Rust toolchain、`protoc`、编译器及相应系统开发库。不使用 `sudo`，不运行 `cargo install`，不修改 `/usr/bin/zellij` 或 pacman 包。

## 当前起点与风险

- 分支：`work/kitty-unicode-placeholders`；fork 的 `origin` 为 `https://github.com/mingerfan/zellij.git`，`upstream` 为 `https://github.com/zellij-org/zellij.git`。
- 起点为上游 `f9ba67d6`，适配 [PR #5531](https://github.com/zellij-org/zellij/pull/5531) 的 Kitty Unicode-placeholder（`U=1`）图片支持，用于 Pi 公式显示。抓取的 PR 为 `3a1af548`，功能提交为 `171152bc`；**普通 Kitty 图片支持不等于 Unicode-placeholder 支持**。
- 本次适配时 PR 尚未合并，仓库版本是 **尚未发布的 0.46.0**，不是系统稳定版 0.45.1；API、配置、插件和图形行为都有变动风险。以上是工作起点记录，不代表以后运行检查时的上游状态。
- 本地维护分支保留该 PR 的提交历史，并额外适配主干的组合字符、缩放缓存和结构化输出。安装前必须完成审查测试并提交（包括 `local/`），再从最终提交重新构建；脚本拒绝未完成的 merge 或脏工作区。

## 构建与安装

从仓库根目录执行：

```sh
git status                       # 先确认最终提交和干净工作区
./local/build.sh
./target/release/zellij --version
./local/install.sh
```

`build.sh` 从 `rust-toolchain.toml` 读取 pin（当前 `1.95.0`），用 `rustup run` 固定工具链，并约束子 Cargo，不使用机器默认 stable。工具链需事先可用，脚本不替你安装它。实际参数为：

```sh
cargo xtask build --release --no-plugins -- --locked --bin zellij \
  --no-default-features --features plugins_from_target,web_server_capability
```

- `CARGO_BUILD_JOBS` 默认 `2`，可用 `CARGO_BUILD_JOBS=4 ./local/build.sh` 覆写。
- 固定输出到仓库 `target/`（覆盖继承的 `CARGO_TARGET_DIR`）。使用本机构建目标，请勿设置交叉编译的 `CARGO_BUILD_TARGET`。
- `--no-plugins` 不重新构建插件；release 继续嵌入已提交的 `zellij-utils/assets/plugins/*.wasm`。不启用默认的 `terminal` feature、不构建 0.46 新增 GUI `zellij-terminal`；保留 web server capability。
- 构建本身不要求工作区干净，便于开发；**安装要求干净且没有未提交 merge**，包括未跟踪文件。不要把旧 `target/release/zellij` 当成当前 HEAD 的产物：每次最终提交后重新构建。`--version` 仅检查可执行程序及版本输出，不能证明二进制对应哪个 commit。

`install.sh` 的默认路径：

```text
~/.local/opt/zellij/<12位 git-shortsha>/bin/zellij
~/.local/opt/zellij/<12位 git-shortsha>/INSTALL.txt
~/.local/bin/zellij -> 上面的 binary
```

说明文件记录版本输出、完整 commit、克隆路径、安装时间及 binary SHA-256。复制后再次校验 `--version`；版本目录不可被未知内容覆盖，同一 commit 的不同 binary 也拒绝覆盖。只替换指向本克隆完整、校验通过版本的项目链接；普通文件、目录、外部/悬空/损坏链接一律拒绝。初次链接排他创建，已有项目链接使用同目录临时链接原子 rename。请串行执行维护脚本，不要在切换期间另行改写安装目录或命令链接。

环境覆写均须为绝对路径，`ZELLIJ_LOCAL_BIN` 指**完整命令路径**而非目录，例如：

```sh
ZELLIJ_LOCAL_PREFIX=/tmp/zellij-test/opt \
ZELLIJ_LOCAL_BIN=/tmp/zellij-test/bin/zellij ./local/install.sh
```

脚本拒绝写入 `/usr/bin`（包括经父目录链接解析后的路径）。确保自己的 shell 已将 `~/.local/bin` 放在 `/usr/bin` 前，再用 `command -v zellij` 和 `zellij --version` 确认；必要时执行 `hash -r`。脚本不会改 shell 配置，也不会自动清理旧版本。

## 测试会话与回退

**更换链接只影响后续启动的命令，不会升级已经运行的 server。** 系统版会话（包括 `mpi_test`）保持原样，不应为安装而杀掉、迁移或重启。当前 client/server contract 仍为 `1`，不能假定版本号不同就会隔离旧 server。

在宿主终端的另一个窗口、而不是现有 Zellij pane 内，选择一个从未用过的会话名测试：

```sh
~/.local/bin/zellij --session "fork-test-$(date +%s)-$$"
```

只在这个新会话验证 Pi 公式、图片、滚动、调整大小和配置；不要直接 attach 旧会话作为升级验证。若需要比独立会话更强的隔离，请另用测试用户/环境。

切换时脚本会输出明确回滚命令，旧版本一直保留。也可以查看 `~/.local/opt/zellij/*/INSTALL.txt`，选择原先安装的目录名（短 SHA 必须与目录名完全一致）：

```sh
./local/install.sh --rollback <旧版本目录的短SHA>
# 使用过路径覆写时，回滚也须传入相同的两个环境变量。
```

回滚只切换链接、校验已保留 binary，不需要当前 `target/` 产物；同样拒绝脏工作区。若工作区脏或要直接回系统版，先确认确实是自己安装的软链接，再删除**链接本身**：

```sh
readlink ~/.local/bin/zellij
# 确认输出指向 ~/.local/opt/zellij/<短SHA>/bin/zellij 后：
rm -- ~/.local/bin/zellij
hash -r
/usr/bin/zellij --version
```

无需删除版本目录，`/usr/bin/zellij` 始终可以显式调用。切换或回退也不会改变已有 server/会话；使用相应 binary 连接对应会话，避免混用新旧客户端。

## 协议验证与边界

本分支针对 Pi 的 `a=T,f=100,q=2,U=1,p=1`、24-bit 前景图片 ID 和逐格显式 row/column 标记增加回归：单块/分块 PNG、默认 placement 选择、两行公式、2027 模式、中文和组合字符、pane 缩窄裁剪、ED3、删除引用、配额和普通图片/等比图片缓存隔离。使用主干的 attached-codepoint 存储，不再借用其他 Plane-16 PUA 字符或使用永久 placeholder interner。

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=2 cargo test --locked \
  -p zellij-server --features web_server_capability --lib -- --test-threads=2
CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=2 cargo test --locked \
  -p zellij-client -p zellij-utils --lib \
  --features zellij-client/web_server_capability -- --test-threads=2
cargo fmt --all --check
# 需要先构建 binary；使用隔离 HOME/XDG/socket 和虚拟宿主终端：
python3 local/pty-smoke.py --compare /usr/bin/zellij
```

PTY smoke 启动真实 client/server 和合成绘图 pane，验证两行 `U=1` PNG 被转换为普通 Kitty 上传与两条 row placement，像素非透明、坐标正确，擦除后不留 stale；同时以系统 0.45.1 作不支持该功能的负对照。它不读取真实 HOME，也不操作既有会话，捕获的终端控制符仅在隔离 PTY 中流转，终端只打印转义 JSON。它验证协议路径，不代替在真正 Kitty 窗口中的目视验收；将来系统版也支持占位符时，应移除或更新 `--compare` 负对照。

图片缩放先检查整数溢出和共享图片配额（沿用上游默认 320 MiB），再分配像素，失败不会无限申请内存。仍持有 virtual placement 的图像占用配额；长会话中大量从未主动删除的图像可能耗尽配额。不要把此实验分支理解为已验证完整 Kitty 协议、所有客户端或所有长会话情形；特别是缺省标记的坐标继承在修改左邻格或重排后还需更广泛验证。Pi 当前逐格显式两项坐标、24-bit ID 的路径已单独覆盖。

## 每周手动检查上游

```sh
./local/check-upstream.sh
```

只 fetch `upstream` 的 `main`（不抓 tags、PR 或其他分支），比较**已提交 HEAD** 与 `upstream/main` 的 merge-base，显示双方独有提交数及最多 20 条上游提交。会更新本地远程跟踪引用；网络错误、缺 remote 或没有共同祖先会报错。不会改 `origin`、当前分支或工作区，不 merge/rebase，不构建/安装，不操作任何会话；尚未提交的 merge 结果不在比较内。

建议每周手动执行一次；先人工审查上游与 PR 状态，再决定如何整合、测试、提交和重建。**不设置自动安装 timer**，也不自动 merge/rebase 或重启会话。

## 离线脚本测试

```sh
bash -n local/*.sh
./local/test.sh
```

测试在 `local/.test.*` 下创建临时 Git 仓库、假 binary、假 rustup 和临时 HOME，结束自动删除；不运行 Cargo、不联网、不使用真实 HOME、不改本仓库 Git 状态。覆盖脏工作区/未提交 merge 拒绝安装、普通文件/未知链接/未知版本拒绝覆盖、版本校验、重复安装、保留旧版本与回滚、损坏版本拒绝回滚、pinned toolchain/参数/jobs，以及用本地 upstream 检查只 fetch main。它不替代 Rust 测试或真实图片测试。
