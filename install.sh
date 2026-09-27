#!/bin/sh
set -eu

repo="iml885203/funliday-cli"
install_dir="${FUNLIDAY_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -s)" in
  Darwin) os="macos" ;;
  Linux) os="linux" ;;
  *) echo "Unsupported operating system. See the README for Windows installation." >&2; exit 1 ;;
esac

case "$(uname -m)" in
  x86_64|amd64) arch="x86_64" ;;
  arm64|aarch64) arch="aarch64" ;;
  *) echo "Unsupported CPU architecture: $(uname -m)" >&2; exit 1 ;;
esac

archive="funliday-${os}-${arch}.tar.gz"
url="https://github.com/${repo}/releases/latest/download/${archive}"
temp_dir="$(mktemp -d)"
trap 'rm -rf "$temp_dir"' EXIT INT TERM

curl --fail --location --proto '=https' --tlsv1.2 "$url" -o "$temp_dir/$archive"
tar -xzf "$temp_dir/$archive" -C "$temp_dir"
mkdir -p "$install_dir"
install -m 0755 "$temp_dir/funliday" "$install_dir/funliday"

echo "Installed funliday to $install_dir/funliday"
case ":$PATH:" in
  *":$install_dir:"*) ;;
  *) echo "Add $install_dir to PATH before running funliday." ;;
esac
