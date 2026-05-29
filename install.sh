#!/bin/sh
set -eu

REPO="${LING_REPO:-LISTENAI/ling}"
BIN="ling"
INSTALL_DIR="${LING_INSTALL_DIR:-$HOME/.local/bin}"
LIBC="${LING_LIBC:-musl}"
TOKEN="${GH_TOKEN:-${GITHUB_TOKEN:-}}"
API_URL="https://api.github.com/repos/$REPO"

say() {
  printf '%s\n' "$*"
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

have() {
  command -v "$1" >/dev/null 2>&1
}

http_get() {
  url="$1"
  if have curl; then
    if [ -n "$TOKEN" ]; then
      curl -fsSL \
        -H "Authorization: Bearer $TOKEN" \
        -H "X-GitHub-Api-Version: 2022-11-28" \
        "$url"
    else
      curl -fsSL "$url"
    fi
  elif have wget; then
    if [ -n "$TOKEN" ]; then
      wget -qO- \
        --header "Authorization: Bearer $TOKEN" \
        --header "X-GitHub-Api-Version: 2022-11-28" \
        "$url"
    else
      wget -qO- "$url"
    fi
  else
    die "curl or wget is required"
  fi
}

http_download() {
  url="$1"
  out="$2"
  if have curl; then
    if [ -n "$TOKEN" ]; then
      curl -fL --progress-bar \
        -H "Authorization: Bearer $TOKEN" \
        -H "X-GitHub-Api-Version: 2022-11-28" \
        -o "$out" "$url"
    else
      curl -fL --progress-bar -o "$out" "$url"
    fi
  elif have wget; then
    if [ -n "$TOKEN" ]; then
      wget -qO "$out" \
        --header "Authorization: Bearer $TOKEN" \
        --header "X-GitHub-Api-Version: 2022-11-28" \
        "$url"
    else
      wget -qO "$out" "$url"
    fi
  else
    die "curl or wget is required"
  fi
}

api_asset_download() {
  asset_id="$1"
  out="$2"
  url="$API_URL/releases/assets/$asset_id"
  if have curl; then
    curl -fL --progress-bar \
      -H "Authorization: Bearer $TOKEN" \
      -H "Accept: application/octet-stream" \
      -H "X-GitHub-Api-Version: 2022-11-28" \
      -o "$out" "$url"
  elif have wget; then
    wget -qO "$out" \
      --header "Authorization: Bearer $TOKEN" \
      --header "Accept: application/octet-stream" \
      --header "X-GitHub-Api-Version: 2022-11-28" \
      "$url"
  else
    die "curl or wget is required"
  fi
}

normalize_arch() {
  case "$1" in
    x86_64|amd64) printf 'x86_64' ;;
    arm64|aarch64) printf 'aarch64' ;;
    *) die "unsupported CPU architecture: $1" ;;
  esac
}

detect_target() {
  os="$(uname -s)"
  arch="$(normalize_arch "$(uname -m)")"

  case "$os" in
    Darwin)
      printf '%s-apple-darwin' "$arch"
      ;;
    Linux)
      case "$LIBC" in
        musl|gnu) ;;
        *) die "unsupported LING_LIBC=$LIBC; expected musl or gnu" ;;
      esac
      printf '%s-unknown-linux-%s' "$arch" "$LIBC"
      ;;
    *)
      die "unsupported OS: $os"
      ;;
  esac
}

latest_version() {
  json="$(http_get "$API_URL/releases/latest")"
  tag="$(printf '%s\n' "$json" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
  [ -n "$tag" ] || die "failed to resolve latest release tag for $REPO"
  printf '%s' "$tag"
}

load_release_json() {
  if [ -z "${release_json:-}" ]; then
    release_json="$(http_get "$API_URL/releases/tags/$version")"
  fi
}

asset_id_for() {
  name="$1"
  load_release_json
  printf '%s\n' "$release_json" | awk -v name="$name" '
    /"id"[[:space:]]*:/ {
      line = $0
      sub(/^[^0-9]*/, "", line)
      sub(/[^0-9].*$/, "", line)
      if (line != "") id = line
    }
    $0 ~ "\"name\"[[:space:]]*:[[:space:]]*\"" name "\"" {
      print id
      exit
    }
  '
}

download_release_asset() {
  name="$1"
  out="$2"
  if [ -n "$TOKEN" ]; then
    id="$(asset_id_for "$name")"
    [ -n "$id" ] || die "release asset not found: $name"
    api_asset_download "$id" "$out"
  else
    http_download "https://github.com/$REPO/releases/download/$version/$name" "$out"
  fi
}

sha256_file() {
  file="$1"
  if have sha256sum; then
    sha256sum "$file" | awk '{print tolower($1)}'
  elif have shasum; then
    shasum -a 256 "$file" | awk '{print tolower($1)}'
  else
    return 1
  fi
}

version="${LING_VERSION:-}"
if [ -z "$version" ]; then
  version="$(latest_version)"
fi

target="$(detect_target)"
asset="$BIN-$version-$target.tar.gz"

tmp="$(mktemp -d 2>/dev/null || mktemp -d -t ling-install)"
cleanup() {
  rm -rf "$tmp"
}
trap cleanup EXIT INT HUP TERM

archive="$tmp/$asset"
checksums="$tmp/SHA256SUMS"

say "Installing $BIN $version for $target"
say "Downloading $asset"
download_release_asset "$asset" "$archive"

if download_release_asset "SHA256SUMS" "$checksums" >/dev/null 2>&1; then
  expected="$(grep "[[:space:]]$asset$" "$checksums" | awk '{print tolower($1)}' | head -n 1)"
  if [ -n "$expected" ]; then
    actual="$(sha256_file "$archive" || true)"
    if [ -n "$actual" ]; then
      [ "$expected" = "$actual" ] || die "checksum mismatch for $asset"
      say "Checksum verified"
    else
      say "Skipping checksum verification: sha256sum or shasum not found"
    fi
  fi
fi

tar -xzf "$archive" -C "$tmp"
src="$tmp/$BIN-$version-$target/$BIN"
[ -f "$src" ] || die "binary not found in archive: $src"

mkdir -p "$INSTALL_DIR"
if have install; then
  install -m 755 "$src" "$INSTALL_DIR/$BIN"
else
  cp "$src" "$INSTALL_DIR/$BIN"
  chmod 755 "$INSTALL_DIR/$BIN"
fi

say "Installed to $INSTALL_DIR/$BIN"
"$INSTALL_DIR/$BIN" --help >/dev/null || die "installed binary failed to run"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    say ""
    say "Add $INSTALL_DIR to PATH if 'ling' is not found:"
    say "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac

say "Done. Try: $BIN --help"
