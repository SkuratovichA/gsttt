#!/bin/sh
# Installs gsttt from a GitHub release, after checking the SHA-256 of the package.
#
#   curl -fsSL https://raw.githubusercontent.com/SkuratovichA/gsttt/main/install.sh | sh
#
# The package is downloaded with gh when gh is installed and signed in, and with curl otherwise.
#
# GSTTT_VERSION      The version to install, such as 0.1.42. The latest release when unset.
# GSTTT_INSTALL_DIR  Where the binary goes. ~/.local/bin when unset.
# GSTTT_BASE_URL     The URL of a directory that holds the packages, https:// or file://, in place of the release. The version then comes from GSTTT_VERSION, from a file named latest in that directory, or, for a local directory, from the newest package for this machine.

set -eu

repo="SkuratovichA/gsttt"

die() {
  printf 'install.sh: %s\n' "$*" >&2
  exit 1
}

have() {
  command -v "$1" > /dev/null 2>&1
}

# The target triple of the packages that run on this machine.
target_triple() {
  os="$(uname -s)"
  arch="$(uname -m)"
  # Under Rosetta uname says x86_64 on an Apple Silicon Mac. The Apple Silicon package runs there natively, and sysctl.proc_translated is 1 only for a translated process (an Intel Mac has no such key).
  if [ "$os" = Darwin ] && [ "$arch" = x86_64 ] && have sysctl && [ "$(sysctl -n sysctl.proc_translated 2> /dev/null)" = 1 ]; then
    arch="arm64"
  fi
  case "$arch" in
    x86_64 | amd64) arch="x86_64" ;;
    aarch64 | arm64) arch="aarch64" ;;
    *) die "there is no package for the architecture $arch" ;;
  esac
  case "$os" in
    Linux) echo "${arch}-unknown-linux-musl" ;;
    Darwin) echo "${arch}-apple-darwin" ;;
    *) die "there is no package for the operating system $os" ;;
  esac
}

# curl for a download. GitHub is fetched over HTTPS only, with TLS 1.2 or newer, a bounded wait and a retry of a transient failure. A base URL may be a local directory or a plain HTTP mirror, so it is fetched without those.
fetch() {
  if [ -n "$base_url" ]; then
    curl -fsSL "$@"
  else
    curl -fsSL --proto '=https' --tlsv1.2 --connect-timeout 15 --max-time 300 --retry 3 "$@"
  fi
}

# Removes what the install made outside the install directory, and the staged copy of the binary if it was not renamed into place.
cleanup() {
  rm -rf "$tmp"
  [ -z "${staged:-}" ] || rm -f "$staged"
}

# The newest version among the packages for the target $2 in the directory $1.
newest_in_directory() {
  for file in "$1"/gsttt-*-"$2".tar.gz; do
    [ -e "$file" ] || continue
    name="${file##*/}"
    name="${name#gsttt-}"
    printf '%s\n' "${name%-"$2".tar.gz}"
  done | sort -t . -k 1,1n -k 2,2n -k 3,3n | tail -n 1
}

# The version of the latest release, as the release says it.
latest_version() {
  if [ "$use_gh" = yes ]; then
    tag="$(gh release view --repo "$repo" --json tagName --jq .tagName)" || die "gh could not find the latest release of $repo"
    echo "$tag"
  elif [ -n "$base_url" ]; then
    if ! fetch "$base_url/latest" 2> /dev/null; then
      case "$base_url" in
        file://*) newest_in_directory "${base_url#file://}" "$target" ;;
        *) die "$base_url/latest does not exist: set GSTTT_VERSION" ;;
      esac
    fi
  else
    url="$(fetch -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest")" || die "could not look up the latest release of $repo: if it is private, sign in with gh auth login"
    tag="${url##*/}"
    case "$tag" in
      v[0-9]*) echo "$tag" ;;
      *) die "$repo has no release yet" ;;
    esac
  fi
}

# The SHA-256 of the file $1.
sha256_of() {
  if have sha256sum; then
    sha256sum "$1" | cut -d ' ' -f 1
  else
    shasum -a 256 "$1" | cut -d ' ' -f 1
  fi
}

main() {
  # Everything the install depends on is looked for before anything is downloaded or made.
  have sha256sum || have shasum || die "sha256sum or shasum is needed to check the download"

  base_url="${GSTTT_BASE_URL:-}"
  base_url="${base_url%/}"
  target="$(target_triple)"

  # A base URL replaces the release, so gh has nothing to ask for.
  use_gh=no
  if [ -z "$base_url" ] && have gh && gh auth status --hostname github.com > /dev/null 2>&1; then
    use_gh=yes
  fi
  if [ "$use_gh" = no ]; then
    have curl || die "curl is needed, or gh signed in to github.com"
  fi

  if [ -n "${GSTTT_VERSION:-}" ]; then
    version="${GSTTT_VERSION#v}"
  else
    version="$(latest_version)"
    version="${version#v}"
  fi
  case "$version" in
    '' | .* | *. | *..* | *[!0-9.]*) die "cannot tell which version to install, got '$version'" ;;
  esac

  if [ -n "${GSTTT_INSTALL_DIR:-}" ]; then
    install_dir="$GSTTT_INSTALL_DIR"
  elif [ -n "${HOME:-}" ]; then
    install_dir="$HOME/.local/bin"
  else
    die "HOME is not set: set GSTTT_INSTALL_DIR"
  fi

  archive="gsttt-${version}-${target}.tar.gz"
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/gsttt-install.XXXXXX")"
  trap cleanup EXIT
  trap 'exit 1' HUP INT TERM

  if [ "$use_gh" = yes ]; then
    gh release download "v${version}" --repo "$repo" --pattern "$archive" --pattern "${archive}.sha256" --dir "$tmp" || die "gh could not download $archive from the release v${version} of $repo"
  else
    release_url="${base_url:-https://github.com/$repo/releases/download/v${version}}"
    for file in "$archive" "${archive}.sha256"; do
      fetch -o "$tmp/$file" "$release_url/$file" || die "could not download $release_url/$file"
    done
  fi

  expected="$(cut -d ' ' -f 1 "$tmp/${archive}.sha256")"
  case "$expected" in
    *[!0-9a-f]*) die "${archive}.sha256 does not start with a SHA-256" ;;
  esac
  [ "${#expected}" -eq 64 ] || die "${archive}.sha256 does not start with a SHA-256"
  actual="$(sha256_of "$tmp/$archive")"
  if [ "$actual" != "$expected" ]; then
    die "the SHA-256 of $archive is $actual, but ${archive}.sha256 says $expected: nothing was installed"
  fi

  mkdir "$tmp/package"
  tar -xzf "$tmp/$archive" -C "$tmp/package"
  [ -f "$tmp/package/gsttt" ] || die "$archive does not hold gsttt"

  # The binary is copied beside its destination, into a file of its own, and renamed into place, so a failed or interrupted copy leaves no half-written gsttt behind: the cleanup on exit removes the copy.
  mkdir -p "$install_dir"
  staged="$(mktemp "$install_dir/.gsttt.XXXXXX")" || die "cannot make a file in $install_dir"
  if ! cp "$tmp/package/gsttt" "$staged" || ! chmod 755 "$staged" || ! mv -f "$staged" "$install_dir/gsttt"; then
    die "could not install gsttt into $install_dir"
  fi

  # A slash at the end of either spelling names the same directory.
  directory="${install_dir%/}"
  printf 'Installed gsttt %s to %s/gsttt\n' "$version" "$directory"
  case ":${PATH:-}:" in
    *":$directory:"* | *":$directory/:"*) ;;
    *)
      printf '%s is not in your PATH. To run gsttt by name, add this line to your shell profile:\n' "$directory"
      printf '  export PATH="%s:%s"\n' "$directory" "\$PATH"
      ;;
  esac
}

main
