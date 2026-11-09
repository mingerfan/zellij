#!/usr/bin/env bash
set -euo pipefail

fail() { echo "install: $*" >&2; exit 1; }
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd -- "$repo"
mode=install
case $# in
    0) ;;
    2) [[ $1 == --rollback && $2 =~ ^[0-9a-f]{7,40}$ ]] || fail "用法: $0 [--rollback <已安装的短 commit>]"
       mode=rollback; id=$2 ;;
    *) fail "用法: $0 [--rollback <已安装的短 commit>]" ;;
esac
[[ $(git rev-parse --show-toplevel) == "$repo" ]] || fail '脚本必须位于仓库 local/ 下'
worktree=$(git status --porcelain --untracked-files=all) || fail '无法检查工作区状态'
[[ -z $worktree ]] || fail '工作区不干净；请先审查并提交（包括 local/），然后重新构建'
if git rev-parse -q --verify MERGE_HEAD >/dev/null; then
    fail '存在未提交 merge；请先完成并提交，再重新构建'
fi

prefix=${ZELLIJ_LOCAL_PREFIX:-"$HOME/.local/opt/zellij"}
link=${ZELLIJ_LOCAL_BIN:-"$HOME/.local/bin/zellij"}
[[ $prefix == /* && $link == /* ]] || fail 'PREFIX 和 BIN 必须是绝对路径（BIN 是完整命令路径）'
prefix=$(realpath -m -- "$prefix")
# 不解析命令链接本身，只规范化它的父目录。
bindir=$(realpath -m -- "$(dirname -- "$link")")
link="$bindir/$(basename -- "$link")"
case "$prefix/" in /usr/bin/*) fail '不能写入 /usr/bin' ;; esac
case "$link" in /usr/bin/*) fail '不能写入 /usr/bin' ;; esac

# 只认可本克隆生成、布局固定、内容未改变的版本；不能仅凭路径在 prefix 内就覆盖。
managed() {
    local target=$1 version_dir short digest
    [[ $target == "$prefix/"*/bin/zellij ]] || return 1
    short=${target#"$prefix/"}; short=${short%/bin/zellij}
    [[ $short =~ ^[0-9a-f]{7,40}$ ]] || return 1
    version_dir="$prefix/$short"
    [[ -d $version_dir && ! -L $version_dir && ! -L $version_dir/bin ]] || return 1
    [[ -f $version_dir/INSTALL.txt && ! -L $version_dir/INSTALL.txt ]] || return 1
    [[ -f $target && -x $target && ! -L $target ]] || return 1
    grep -Fxq 'zellij-local-install-v1' "$version_dir/INSTALL.txt" || return 1
    grep -Fxq "repository=$repo" "$version_dir/INSTALL.txt" || return 1
    grep -Fxq "short_commit=$short" "$version_dir/INSTALL.txt" || return 1
    digest=$(sha256sum -- "$target") || return 1
    digest=${digest%% *}
    grep -Fxq "binary_sha256=$digest" "$version_dir/INSTALL.txt"
}

old=
if [[ -L $link ]]; then
    old=$(readlink -- "$link")
    [[ $old == /* ]] || old="$bindir/$old"
    old=$(realpath -m -- "$old")
    managed "$old" || fail "拒绝替换非本项目或已损坏的软链接: $link"
elif [[ -e $link ]]; then
    fail "拒绝覆盖已有普通文件或目录: $link"
fi

stage= link_stage=
cleanup() {
    [[ -z $stage ]] || rm -rf -- "$stage"
    [[ -z $link_stage ]] || rm -rf -- "$link_stage"
    return 0
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

if [[ $mode == install ]]; then
    commit=$(git rev-parse HEAD)
    id=$(git rev-parse --short=12 HEAD)
    source="$repo/target/release/zellij"
    [[ -f $source && -x $source && ! -L $source ]] || fail "缺少可执行构建产物: $source"
    version=$("$source" --version) || fail '构建产物 --version 失败'
    [[ $version =~ ^zellij\ [0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]] || fail "异常版本输出: $version"
    target="$prefix/$id/bin/zellij"
    if [[ -e $prefix/$id || -L $prefix/$id ]]; then
        managed "$target" || fail "拒绝覆盖未知版本目录: $prefix/$id"
        grep -Fxq "commit=$commit" "$prefix/$id/INSTALL.txt" || fail '短 commit 冲突'
        cmp -s -- "$source" "$target" || fail '同一 commit 已有不同 binary；保留旧版本，不覆盖'
    else
        mkdir -p -- "$prefix"
        stage=$(mktemp -d "$prefix/.install.XXXXXX")
        mkdir -- "$stage/bin"
        install -m 755 -- "$source" "$stage/bin/zellij"
        [[ $("$stage/bin/zellij" --version) == "$version" ]] || fail '复制后版本校验失败'
        digest=$(sha256sum -- "$stage/bin/zellij"); digest=${digest%% *}
        {
            printf 'zellij-local-install-v1\nrepository=%s\ncommit=%s\nshort_commit=%s\n' "$repo" "$commit" "$id"
            printf 'version=%s\nbinary_sha256=%s\ninstalled_utc=%s\n' "$version" "$digest" "$(date -u +%FT%TZ)"
        } > "$stage/INSTALL.txt"
        mv -Tn -- "$stage" "$prefix/$id"
        [[ ! -d $stage ]] || fail '安装期间版本目录已出现，拒绝覆盖'
        stage=
    fi
else
    target="$prefix/$id/bin/zellij"
    managed "$target" || fail "不是本克隆的完整已安装版本: $id"
    version=$("$target" --version) || fail '回滚版本 --version 失败'
    [[ $version =~ ^zellij\ [0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]] || fail "异常版本输出: $version"
fi

if [[ $old != "$target" ]]; then
    mkdir -p -- "$bindir"
    if [[ -z $old ]]; then
        # 不存在时用 ln 的原子排他创建，绝不覆盖期间出现的其他文件。
        ln -sT -- "$target" "$link"
    else
        link_stage=$(mktemp -d "$bindir/.zellij-link.XXXXXX")
        ln -s -- "$target" "$link_stage/zellij"
        current=$(readlink -f -- "$link") || fail '切换期间命令链接发生变化'
        [[ -L $link && $current == "$old" ]] || fail '切换期间命令链接发生变化'
        mv -Tf -- "$link_stage/zellij" "$link"
    fi
fi
printf '%s -> %s\n%s\n' "$link" "$target" "$version"
if [[ -n $old && $old != "$target" ]]; then
    previous=${old#"$prefix/"}; previous=${previous%/bin/zellij}
    printf '回滚命令: ZELLIJ_LOCAL_PREFIX=%q ZELLIJ_LOCAL_BIN=%q %q --rollback %q\n' \
        "$prefix" "$link" "$repo/local/install.sh" "$previous"
fi
