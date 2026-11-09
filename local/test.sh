#!/usr/bin/env bash
# 全部状态留在 local/ 下的临时目录；不访问网络、真实 HOME，也不运行 Cargo。
set -euo pipefail
scripts=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
tmp=$(mktemp -d "$scripts/.test.XXXXXX")
trap 'rm -rf -- "$tmp"' EXIT
export HOME="$tmp/home" XDG_CONFIG_HOME="$tmp/home/.config"
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_TERMINAL_PROMPT=0
export GIT_TEMPLATE_DIR="$tmp/empty-template"
# 不继承调用者的 Git 仓库/索引/配置覆写或自定义模板 hooks。
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES
unset GIT_CONFIG GIT_CONFIG_COUNT GIT_CONFIG_PARAMETERS
repo="$tmp/repo"
mkdir -p -- "$repo/local" "$HOME" "$tmp/mocks" "$GIT_TEMPLATE_DIR"
# 即使脚本回归误调用 Cargo，也只能命中阻断桩。
printf '#!/bin/sh\necho "test: 禁止运行真实 Cargo" >&2\nexit 97\n' > "$tmp/mocks/cargo"
chmod +x "$tmp/mocks/cargo"
export PATH="$tmp/mocks:$PATH"
cp -- "$scripts/"{build,install,check-upstream}.sh "$repo/local/"
cd -- "$repo"
git init --quiet -b main
git config user.name 'Local Script Test'
git config user.email 'local-test@example.invalid'
printf 'target/\n' > .gitignore
printf 'source one\n' > source.txt
printf '[toolchain]\nchannel = "1.95.0"\n' > rust-toolchain.toml
git add .
git commit --quiet -m 'test source one'
export ZELLIJ_LOCAL_PREFIX="$tmp/opt/zellij" ZELLIJ_LOCAL_BIN="$tmp/bin/zellij"
installer="$repo/local/install.sh"

fail() { echo "FAIL: $*" >&2; exit 1; }
reject() {
    local message=$1; shift
    if "$@" > "$tmp/result" 2>&1; then fail "应拒绝: $message"; fi
    grep -Fq -- "$message" "$tmp/result" || { printf '意外错误:\n' >&2; tail -20 "$tmp/result" >&2; fail "$message"; }
}
assert_link() { [[ $(readlink -- "$ZELLIJ_LOCAL_BIN") == "$ZELLIJ_LOCAL_PREFIX/$1/bin/zellij" ]] || fail "链接目标不是 $1"; }
fake_binary() {
    mkdir -p target/release
    printf '#!/bin/sh\n# build %s\n[ "$1" = --version ] || exit 9\nprintf "zellij 0.46.0\\n"\n' "$1" > target/release/zellij
    chmod +x target/release/zellij
}
fake_binary one
id1=$(git rev-parse --short=12 HEAD)

# 未跟踪、已暂存改动及即使 status 为空的未提交 merge 都应拒绝。
printf 'dirty\n' > untracked
reject '工作区不干净' "$installer"
[[ ! -e $ZELLIJ_LOCAL_PREFIX && ! -e $ZELLIJ_LOCAL_BIN ]] || fail '脏工作区产生了安装状态'
rm untracked
printf 'dirty\n' >> source.txt
git add source.txt
reject '工作区不干净' "$installer"
git restore --staged --worktree source.txt
git rev-parse HEAD > .git/MERGE_HEAD
reject '未提交 merge' "$installer"
rm .git/MERGE_HEAD

# 不覆盖普通文件、目录、外部软链接、悬空链接或伪装成 prefix 内版本的链接。
mkdir -p -- "$(dirname -- "$ZELLIJ_LOCAL_BIN")"
printf 'keep me\n' > "$ZELLIJ_LOCAL_BIN"
reject '拒绝覆盖已有普通文件或目录' "$installer"
[[ $(< "$ZELLIJ_LOCAL_BIN") == 'keep me' ]] || fail '普通文件被覆盖'
rm -- "$ZELLIJ_LOCAL_BIN"
mkdir -- "$ZELLIJ_LOCAL_BIN"
reject '拒绝覆盖已有普通文件或目录' "$installer"
rmdir -- "$ZELLIJ_LOCAL_BIN"
printf 'external\n' > "$tmp/external"
for foreign in "$tmp/external" "$tmp/missing" "$ZELLIJ_LOCAL_PREFIX/aaaaaaaaaaaa/bin/zellij"; do
    ln -s -- "$foreign" "$ZELLIJ_LOCAL_BIN"
    reject '拒绝替换非本项目或已损坏的软链接' "$installer"
    [[ $(readlink -- "$ZELLIJ_LOCAL_BIN") == "$foreign" ]] || fail '外部链接被覆盖'
    rm -- "$ZELLIJ_LOCAL_BIN"
done
mkdir -p -- "$ZELLIJ_LOCAL_PREFIX/$id1"
printf 'unknown\n' > "$ZELLIJ_LOCAL_PREFIX/$id1/keep"
reject '拒绝覆盖未知版本目录' "$installer"
[[ $(< "$ZELLIJ_LOCAL_PREFIX/$id1/keep") == unknown ]] || fail '未知版本被覆盖'
rm -r -- "$ZELLIJ_LOCAL_PREFIX/$id1"
printf '#!/bin/sh\nexit 1\n' > target/release/zellij
reject '构建产物 --version 失败' "$installer"
printf '#!/bin/sh\nprintf "not-zellij\\n"\n' > target/release/zellij
reject '异常版本输出' "$installer"
fake_binary one

# 初装、相同版本重跑、不同 binary 的同 commit 不得覆盖。
"$installer" > "$tmp/result"
assert_link "$id1"
cmp -- target/release/zellij "$ZELLIJ_LOCAL_BIN" || fail '复制内容不同'
grep -Fxq "commit=$(git rev-parse HEAD)" "$ZELLIJ_LOCAL_PREFIX/$id1/INSTALL.txt"
grep -Fxq 'version=zellij 0.46.0' "$ZELLIJ_LOCAL_PREFIX/$id1/INSTALL.txt"
"$installer" > "$tmp/result"
fake_binary altered
reject '同一 commit 已有不同 binary' "$installer"
assert_link "$id1"

# 新 commit 切换后明确给出旧 commit 回滚命令；回滚不需要 target 产物。
printf 'source two\n' >> source.txt
git add source.txt
git commit --quiet -m 'test source two'
id2=$(git rev-parse --short=12 HEAD)
fake_binary two
"$installer" > "$tmp/result"
assert_link "$id2"
grep -Fq -- "--rollback $id1" "$tmp/result"
[[ -x $ZELLIJ_LOCAL_PREFIX/$id1/bin/zellij ]] || fail '旧版本未保留'
rm target/release/zellij
printf 'dirty\n' > untracked
reject '工作区不干净' "$installer" --rollback "$id1"
assert_link "$id2"
rm untracked
"$installer" --rollback "$id1" > "$tmp/result"
assert_link "$id1"
"$installer" --rollback "$id2" > "$tmp/result"
assert_link "$id2"
reject '不是本克隆的完整已安装版本' "$installer" --rollback bbbbbbbbbbbb
assert_link "$id2"
printf '\n# tampered\n' >> "$ZELLIJ_LOCAL_PREFIX/$id1/bin/zellij"
reject '不是本克隆的完整已安装版本' "$installer" --rollback "$id1"
assert_link "$id2"
echo 'PASS: install 拒绝覆盖、版本校验、切换与回滚'

# 用假的 rustup 检查构建参数和环境；绝不调用真实 rustup/cargo。
printf '#!/bin/sh\nprintf "%%s\\n" "$@" > "$TEST_ARGS"\nprintf "%%s\\n" "$RUSTUP_TOOLCHAIN" "$CARGO_BUILD_JOBS" "$CARGO_TARGET_DIR" > "$TEST_ENV"\n' > "$tmp/mocks/rustup"
chmod +x "$tmp/mocks/rustup"
export TEST_ARGS="$tmp/args" TEST_ENV="$tmp/env"
RUSTUP_TOOLCHAIN=wrong CARGO_BUILD_JOBS=7 "$repo/local/build.sh"
printf '%s\n' run 1.95.0 cargo xtask build --release --no-plugins -- --locked --bin zellij --no-default-features --features plugins_from_target,web_server_capability > "$tmp/expected-args"
cmp "$TEST_ARGS" "$tmp/expected-args"
printf '%s\n' 1.95.0 7 "$repo/target" > "$tmp/expected-env"
cmp "$TEST_ENV" "$tmp/expected-env"
env -u CARGO_BUILD_JOBS "$repo/local/build.sh"
printf '%s\n' 1.95.0 2 "$repo/target" > "$tmp/expected-env"
cmp "$TEST_ENV" "$tmp/expected-env"
echo 'PASS: pinned toolchain、构建参数与 jobs 默认/覆写'

# upstream 是本地临时仓库，不连接网络；额外分支和 tags 不应被抓取。
git clone --quiet --no-hardlinks "$repo" "$tmp/upstream"
git -C "$tmp/upstream" config user.name 'Local Script Test'
git -C "$tmp/upstream" config user.email 'local-test@example.invalid'
printf 'upstream change\n' >> "$tmp/upstream/source.txt"
git -C "$tmp/upstream" add source.txt
git -C "$tmp/upstream" commit --quiet -m 'upstream change'
git -C "$tmp/upstream" branch other
git -C "$tmp/upstream" tag test-tag
git remote add upstream "$tmp/upstream"
git remote add origin "$tmp/untouched-origin"
origin_before=$(git remote get-url origin)
head_before=$(git rev-parse HEAD)
status_before=$(git status --porcelain)
"$repo/local/check-upstream.sh" > "$tmp/result" 2>&1
grep -Fq '当前分支尚未包含的上游提交: 1' "$tmp/result"
[[ $(git rev-parse HEAD) == "$head_before" && $(git status --porcelain) == "$status_before" ]] || fail '检查改变了分支或工作区'
[[ $(git remote get-url origin) == "$origin_before" ]] || fail 'origin 被修改'
[[ $(git rev-parse refs/remotes/upstream/main) == "$(git -C "$tmp/upstream" rev-parse HEAD)" ]] || fail 'upstream/main 未更新'
if git show-ref --verify --quiet refs/remotes/upstream/other || git show-ref --verify --quiet refs/tags/test-tag; then fail '抓取了 main 以外的引用'; fi
echo 'PASS: 只 fetch upstream/main，不改 origin/HEAD/工作区'
