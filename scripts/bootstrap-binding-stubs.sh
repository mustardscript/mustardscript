#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_root="${repo_root}/tmp/npm-binding-bootstrap"
scope="@mustardscript"
version="0.0.0-bootstrap"
publish="false"
dry_run="false"

usage() {
  cat <<'EOF'
Usage: scripts/bootstrap-binding-stubs.sh [options]

Generate minimal scoped npm binding-package stubs for Trusted Publishing setup.

Options:
  --output-dir <path>  Directory to write the stub packages into.
  --scope <scope>      npm scope to use. Default: @mustardscript
  --version <version>  Stub version to publish. Default: 0.0.0-bootstrap
  --publish            Run npm publish --access public for each generated stub.
  --dry-run            With --publish, run npm publish --dry-run --access public.
  --help               Show this help text.

Examples:
  scripts/bootstrap-binding-stubs.sh
  scripts/bootstrap-binding-stubs.sh --publish
  scripts/bootstrap-binding-stubs.sh --output-dir /tmp/ms-bootstrap --publish
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output-dir)
      output_root="$2"
      shift 2
      ;;
    --scope)
      scope="$2"
      shift 2
      ;;
    --version)
      version="$2"
      shift 2
      ;;
    --publish)
      publish="true"
      shift
      ;;
    --dry-run)
      dry_run="true"
      shift
      ;;
    --help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ "${scope}" != @* ]]; then
  echo "--scope must start with @" >&2
  exit 1
fi

mkdir -p "${output_root}"

write_stub() {
  local package_suffix="$1"
  local os_name="$2"
  local cpu_name="$3"
  local libc_name="${4:-}"
  local package_name="${scope}/${package_suffix}"
  local package_dir="${output_root}/${package_suffix}"

  mkdir -p "${package_dir}"

  cat > "${package_dir}/placeholder.js" <<EOF
'use strict';

throw new Error(
  'Bootstrap package only: ${package_name}@${version} does not contain a native binary yet.',
);
EOF

  cat > "${package_dir}/README.md" <<EOF
# \`${package_name}\`

Bootstrap package for npm Trusted Publishing setup. This placeholder release is
only meant to reserve the package name and configure npm package settings
before the first real native binary publish.
EOF

  if [[ -n "${libc_name}" ]]; then
    cat > "${package_dir}/package.json" <<EOF
{
  "name": "${package_name}",
  "version": "${version}",
  "private": false,
  "description": "Bootstrap package for npm Trusted Publishing setup",
  "license": "Apache-2.0",
  "os": ["${os_name}"],
  "cpu": ["${cpu_name}"],
  "libc": ["${libc_name}"],
  "files": ["README.md", "placeholder.js"],
  "main": "placeholder.js",
  "publishConfig": {
    "access": "public"
  }
}
EOF
  else
    cat > "${package_dir}/package.json" <<EOF
{
  "name": "${package_name}",
  "version": "${version}",
  "private": false,
  "description": "Bootstrap package for npm Trusted Publishing setup",
  "license": "Apache-2.0",
  "os": ["${os_name}"],
  "cpu": ["${cpu_name}"],
  "files": ["README.md", "placeholder.js"],
  "main": "placeholder.js",
  "publishConfig": {
    "access": "public"
  }
}
EOF
  fi

  echo "Wrote ${package_dir}"

  if [[ "${publish}" == "true" ]]; then
    if [[ "${dry_run}" == "true" ]]; then
      (
        cd "${package_dir}"
        npm publish --dry-run --access public
      )
    else
      (
        cd "${package_dir}"
        npm publish --access public
      )
    fi
  fi
}

write_stub "binding-darwin-arm64" "darwin" "arm64"
write_stub "binding-darwin-x64" "darwin" "x64"
write_stub "binding-linux-x64-gnu" "linux" "x64" "glibc"
write_stub "binding-win32-x64-msvc" "win32" "x64"

echo
echo "Stub packages are in ${output_root}"
if [[ "${publish}" == "true" ]]; then
  if [[ "${dry_run}" == "true" ]]; then
    echo "Ran npm publish --dry-run for each stub package."
  else
    echo "Published each stub package."
  fi
else
  echo "Review the generated package.json files, then publish each package with:"
  echo "  cd <package-dir> && npm publish --access public"
fi
