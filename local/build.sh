#!/usr/bin/env bash
set -euo pipefail

repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd -- "$repo"
[[ $# == 0 ]] || { echo "用法: $0" >&2; exit 1; }
toolchain=$(awk -F '"' '/^[[:space:]]*channel[[:space:]]*=/ { print $2; exit }' rust-toolchain.toml)
[[ -n "$toolchain" ]] || { echo '无法读取仓库 pinned toolchain' >&2; exit 1; }

# 同时约束 xtask 启动的子 cargo；不使用机器默认 stable。
export RUSTUP_TOOLCHAIN="$toolchain"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_TARGET_DIR="$repo/target"
exec rustup run "$toolchain" cargo xtask build --release --no-plugins -- \
    --locked --bin zellij --no-default-features \
    --features plugins_from_target,web_server_capability
