#!/usr/bin/env bash
set -euo pipefail

repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd -- "$repo"
[[ $# == 0 ]] || { echo "用法: $0" >&2; exit 1; }
# 显式 refspec：不抓取 PR、其他分支或 tags，也不依赖 remote 的 fetch 配置。
git fetch --no-tags --no-recurse-submodules --no-auto-maintenance \
    upstream +refs/heads/main:refs/remotes/upstream/main
base=$(git merge-base HEAD refs/remotes/upstream/main)
read -r local_only upstream_only < <(git rev-list --left-right --count HEAD...refs/remotes/upstream/main)
printf 'HEAD:          %s\nupstream/main: %s\nmerge-base:    %s\n' \
    "$(git rev-parse --short=12 HEAD)" "$(git rev-parse --short=12 refs/remotes/upstream/main)" "$(git rev-parse --short=12 "$base")"
printf '仅当前分支: %s 个提交；当前分支尚未包含的上游提交: %s\n' "$local_only" "$upstream_only"
if [[ $upstream_only != 0 ]]; then
    git --no-pager log --oneline --no-decorate -20 "$base..refs/remotes/upstream/main"
fi
printf '仅检查已提交的 HEAD；未合并、变基、构建、安装或操作运行中的会话。\n'
