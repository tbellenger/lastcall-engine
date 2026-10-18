#!/usr/bin/env bash
# Build a self-contained Charlie web player: wasm, JS shell, package, and page.
# Usage: build-web-bundle.sh [output.tgz]
# Default output: dist/charlie-next-web.tgz
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

out="${1:-dist/charlie-next-web.tgz}"
case "$out" in
  /*) ;;
  *) out="$root/$out" ;;
esac

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "wasm-bindgen is not installed. CI installs 0.2.100 with:" >&2
  echo "  cargo install wasm-bindgen-cli --version 0.2.100 --locked" >&2
  exit 1
fi

bash scripts/build-wasm.sh
cargo run -p advc --release -- pack fixtures/charlie -o web/public/charlie.advpak

(
  cd web
  npm ci
  npm run build:bundle
)

bundle="$root/web/dist-bundle"
test -f "$bundle/player.js"
test -f "$bundle/player.css"
if ! find "$bundle" -name '*.wasm' -print -quit | grep -q .; then
  echo "vite did not emit a wasm asset" >&2
  exit 1
fi

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
dest="$stage/charlie-next"
mkdir -p "$dest"
cp -a "$bundle/." "$dest/"
cp "$root/web/public/charlie.advpak" "$dest/charlie.advpak"
cp "$root/web/public/charlie-title.png" "$dest/charlie-title.png"
cp "$root/packaging/charlie-next/index.html" "$dest/index.html"
cp "$root/packaging/charlie-next/README.md" "$dest/README.md"

python3 - "$dest" <<'PY'
import pathlib, sys
root = pathlib.Path(sys.argv[1])
required = [
    "README.md",
    "index.html",
    "player.js",
    "player.css",
    "charlie.advpak",
    "charlie-title.png",
]
missing = [name for name in required if not (root / name).is_file()]
wasm = list(root.rglob("*.wasm"))
if not wasm:
    missing.append("*.wasm")
if missing:
    raise SystemExit("bundle is missing: " + ", ".join(missing))

readme = (root / "README.md").read_text(encoding="utf-8")
for phrase in ("mountPlayer", "application/wasm", "lastcall-charlie-next", "basePath"):
    if phrase not in readme:
        raise SystemExit(f"README.md does not mention {phrase}")

player = (root / "player.js").read_text(encoding="utf-8")
for phrase in ("mountPlayer", "lastcall-charlie-next"):
    if phrase not in player:
        raise SystemExit(f"player.js does not mention {phrase}")

import re
bad = re.compile(r"""(?:(?:src|href)\s*=\s*["']/(?!/)|url\(\s*/|fetch\(\s*["']/|from\s+["']/|import\(\s*["']/|new URL\(\s*["']/)""")
scanned = [root / "index.html", root / "player.js", root / "player.css"]
scanned.extend(p for p in root.rglob("*.js") if p.name != "player.js")
for path in scanned:
    text = path.read_text(encoding="utf-8", errors="replace")
    match = bad.search(text)
    if match:
        raise SystemExit(f"root-relative URL in {path.name}: {match.group(0)}")
print(f"wasm: {wasm[0].relative_to(root)}")
PY

mkdir -p "$(dirname "$out")"
rm -f "$out"
tar -C "$stage" -czf "$out" charlie-next
echo "wrote $out"
