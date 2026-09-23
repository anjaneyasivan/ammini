#!/usr/bin/env bash
#
# Build a self-contained Ammini.app and Ammini-<version>.dmg for macOS.
#
# libmpv and its entire dylib closure are copied into Contents/Frameworks with
# their install names rewritten to @rpath, so the bundle runs on Macs that do
# not have Homebrew's mpv installed. Everything is ad-hoc signed, which is
# enough for local use and for sharing (recipients may need
# "right-click -> Open" the first time). Shipping to the public without a
# Gatekeeper warning needs a Developer ID signature + notarization, which this
# script deliberately does not do.
#
# Usage: scripts/bundle-macos.sh
set -euo pipefail

cd "$(dirname "$0")/.."

APP_NAME="Ammini"
BUNDLE_ID="com.ammini.player"
MIN_MACOS="11.0"
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)"
: "${VERSION:=0.1.0}"

DIST="dist"
APP="$DIST/$APP_NAME.app"
CONTENTS="$APP/Contents"
MACOS_DIR="$CONTENTS/MacOS"
RESOURCES_DIR="$CONTENTS/Resources"
FRAMEWORKS_DIR="$CONTENTS/Frameworks"
BIN_SRC="target/release/ammini"
BIN_DST="$MACOS_DIR/$APP_NAME"

echo "==> cargo build --release"
cargo build --release

echo "==> Assembling $APP"
rm -rf "$APP"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR" "$FRAMEWORKS_DIR"
cp "$BIN_SRC" "$BIN_DST"
cp "assets/$APP_NAME.icns" "$RESOURCES_DIR/AppIcon.icns"

cat > "$CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleExecutable</key>
	<string>$APP_NAME</string>
	<key>CFBundleIdentifier</key>
	<string>$BUNDLE_ID</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>$APP_NAME</string>
	<key>CFBundleDisplayName</key>
	<string>$APP_NAME</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>$VERSION</string>
	<key>CFBundleVersion</key>
	<string>$VERSION</string>
	<key>CFBundleIconFile</key>
	<string>AppIcon</string>
	<key>LSMinimumSystemVersion</key>
	<string>$MIN_MACOS</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSPrincipalClass</key>
	<string>NSApplication</string>
</dict>
</plist>
PLIST

echo "==> Bundling libmpv and its dylib closure into Contents/Frameworks"
python3 - "$BIN_DST" "$FRAMEWORKS_DIR" <<'PY'
import os, shutil, subprocess, sys

binary, frameworks = sys.argv[1], sys.argv[2]


def deps(path):
    out = subprocess.run(["otool", "-L", path], capture_output=True, text=True, check=True).stdout
    return [line.strip().split(" (")[0] for line in out.splitlines()[1:]]


def is_third_party(path):
    return path.startswith(("/opt/homebrew/", "/usr/local/"))


# Key copies by the basename used in the Mach-O install names, since that is what
# the rewritten @rpath entries will look for. Homebrew exposes e.g.
# libavformat.63.dylib as a symlink to libavformat.63.1.102.dylib, so naming the
# copy after the realpath would not match the install name.
by_name = {}
queue = deps(binary)
while queue:
    lib = queue.pop()
    if not is_third_party(lib) or not os.path.exists(lib):
        continue
    base = os.path.basename(lib)
    if base in by_name:
        if os.path.realpath(by_name[base]) != os.path.realpath(lib):
            sys.exit(f"dylib basename collision: {base}\n  {by_name[base]}\n  {lib}")
        continue
    by_name[base] = lib
    queue.extend(deps(lib))

for base, src in sorted(by_name.items()):
    shutil.copy2(src, os.path.join(frameworks, base))

total = sum(os.path.getsize(src) for src in by_name.values())
print(f"    copied {len(by_name)} dylibs ({total / 1e6:.1f} MB)")
PY

echo "==> Rewriting install names to @rpath"
# Homebrew ships dylibs read-only; make the copies writable so install_name_tool
# can edit them.
chmod u+w "$BIN_DST" "$FRAMEWORKS_DIR"/*.dylib
for file in "$BIN_DST" "$FRAMEWORKS_DIR"/*.dylib; do
    otool -L "$file" | tail -n +2 | sed -e 's/^[[:space:]]*//' -e 's/ (.*//' | while IFS= read -r dep; do
        case "$dep" in
            /opt/homebrew/* | /usr/local/*)
                install_name_tool -change "$dep" "@rpath/$(basename "$dep")" "$file"
                ;;
        esac
    done
done

for lib in "$FRAMEWORKS_DIR"/*.dylib; do
    install_name_tool -id "@rpath/$(basename "$lib")" "$lib"
    install_name_tool -add_rpath "@loader_path" "$lib" 2>/dev/null || true
done

# The executable resolves its libraries from Frameworks/; drop the build-time
# Homebrew rpath so a broken dependency can never fall back to a system copy.
install_name_tool -delete_rpath /opt/homebrew/lib "$BIN_DST" 2>/dev/null || true
install_name_tool -delete_rpath /usr/local/lib "$BIN_DST" 2>/dev/null || true
install_name_tool -add_rpath "@executable_path/../Frameworks" "$BIN_DST" 2>/dev/null || true

echo "==> Verifying the bundle is self-contained"
python3 - "$CONTENTS" "$FRAMEWORKS_DIR" <<'PY'
import os, subprocess, sys

contents, frameworks = sys.argv[1], sys.argv[2]
problems = []

for root, _, files in os.walk(contents):
    for name in files:
        path = os.path.join(root, name)
        result = subprocess.run(["otool", "-L", path], capture_output=True, text=True)
        if result.returncode != 0:
            continue  # not a Mach-O (Info.plist, .icns, ...)
        for line in result.stdout.splitlines()[1:]:
            dep = line.strip().split(" (")[0]
            if dep.startswith("@rpath/"):
                if not os.path.exists(os.path.join(frameworks, os.path.basename(dep))):
                    problems.append(f"{dep} (needed by {os.path.relpath(path, contents)})")
            elif dep.startswith(("/opt/homebrew/", "/usr/local/")):
                problems.append(f"unrewritten Homebrew path {dep} (in {os.path.relpath(path, contents)})")

if problems:
    print("    bundle is NOT self-contained:", file=sys.stderr)
    for p in problems:
        print(f"      {p}", file=sys.stderr)
    sys.exit(1)

print("    all @rpath references resolve; no Homebrew paths remain")
PY

echo "==> Signing (ad-hoc)"
for lib in "$FRAMEWORKS_DIR"/*.dylib; do
    codesign --force --sign - "$lib" >/dev/null 2>&1
done
codesign --force --sign - "$APP"
codesign --verify --strict "$APP"

echo "==> Building DMG"
DMG="$DIST/$APP_NAME-$VERSION.dmg"
rm -f "$DMG"
STAGE="$(mktemp -d)"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "$APP_NAME" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"

echo
echo "App: $APP"
echo "DMG: $DMG ($(du -h "$DMG" | cut -f1))"
echo
echo "Run it now:   open \"$APP\""
echo "Install it:   open \"$DMG\"   # then drag Ammini into Applications"
