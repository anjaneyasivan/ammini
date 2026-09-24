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
# IMPORTANT: Homebrew builds its libraries for the *build* machine's macOS, and the
# bundle inherits that floor. Building on macOS 27 produces an app that only runs on
# macOS 27+; on an older Mac, dyld aborts at launch with a "Symbol missing" error
# (e.g. libglib referencing _pipe2). Build on the oldest macOS you intend to support.
# The script reports the real floor and writes it into LSMinimumSystemVersion.
# Set MIN_MACOS to the oldest macOS the app must run on (e.g. MIN_MACOS=15.5
# ./scripts/bundle-macos.sh); the script flags it before building if this machine
# cannot deliver that floor, since no rewriting can lower it.
#
# Usage: scripts/bundle-macos.sh
set -euo pipefail

cd "$(dirname "$0")/.."

APP_NAME="Ammini"
BUNDLE_ID="com.ammini.player"
# Oldest macOS the bundle must run on. Defaults to the build machine's macOS, the
# lowest floor this machine can produce; override when building on the target OS,
# e.g. MIN_MACOS=15.5 ./scripts/bundle-macos.sh, so the floor checks use it.
: "${MIN_MACOS:=$(sw_vers -productVersion)}"
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

BUILD_MACOS="$(sw_vers -productVersion)"
echo "==> Host macOS: $BUILD_MACOS"

# Homebrew compiles its dylibs for the build machine, so the bundle's floor is the
# build machine's macOS, never lower. If the caller declared a lower floor than this
# machine can produce, say so before the release build (the end-of-script floor check
# - and dyld, on the target Mac - would otherwise be the first to complain).
NEWEST="$(printf '%s\n%s\n' "$BUILD_MACOS" "$MIN_MACOS" | sort -V | tail -1)"
if [ "$NEWEST" = "$BUILD_MACOS" ] && [ "$BUILD_MACOS" != "$MIN_MACOS" ]; then
    echo
    echo "WARNING: this machine is macOS $BUILD_MACOS but MIN_MACOS is set to $MIN_MACOS."
    echo "  A bundle built here requires macOS $BUILD_MACOS and aborts on older systems"
    echo "  (dyld \"Symbol missing\"). To get a bundle that runs on macOS $MIN_MACOS, build"
    echo "  it on macOS $MIN_MACOS or older: an older Mac, a macOS VM, or a CI runner on"
    echo "  that OS (e.g. a macos-15 GitHub Actions runner with 'brew install mpv')."
    echo "  Proceeding produces an app that only runs on macOS $BUILD_MACOS+."
    echo
fi

echo "==> cargo build --release"
cargo build --release

echo "==> Assembling $APP"
rm -rf "$APP"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR" "$FRAMEWORKS_DIR"
cp "$BIN_SRC" "$BIN_DST"
cp "assets/$APP_NAME.icns" "$RESOURCES_DIR/AppIcon.icns"

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

echo "==> Determining the minimum macOS version"
# A bundled dylib built for a newer macOS than the machine that runs it makes dyld
# abort at launch (e.g. libglib referencing _pipe2, absent before macOS 27). Homebrew
# builds its libraries for the *build* machine's OS, so a bundle produced on a new
# macOS only runs on that macOS or newer. Report the real floor and put it in the
# plist, so older systems get a clear "requires macOS X" dialog instead of a crash.
ACTUAL_MIN="$(python3 - "$CONTENTS" "$MIN_MACOS" <<'PY'
import os, subprocess, sys

contents, target = sys.argv[1], sys.argv[2]


def deployment_target(path):
    out = subprocess.run(["otool", "-l", path], capture_output=True, text=True)
    if out.returncode != 0:
        return None
    lines = out.stdout.splitlines()
    for i, line in enumerate(lines):
        if "LC_BUILD_VERSION" in line:
            for j in range(i, min(i + 6, len(lines))):
                if "minos" in lines[j]:
                    return lines[j].split()[1]
        elif "LC_VERSION_MIN_MACOSX" in line:
            for j in range(i, min(i + 4, len(lines))):
                if "version" in lines[j]:
                    return lines[j].split()[1]
    return None


def as_key(v):
    return tuple(int(part) for part in v.split("."))


found = []
for root, _, files in os.walk(contents):
    for name in files:
        path = os.path.join(root, name)
        v = deployment_target(path)
        if v:
            found.append((v, os.path.relpath(path, contents)))

if not found:
    sys.exit("could not determine the deployment target of any bundled Mach-O")

worst, worst_path = max(found, key=lambda item: as_key(item[0]))
print(f"    highest deployment target: {worst} ({worst_path})", file=sys.stderr)
if as_key(worst) > as_key(target):
    print(
        f"    WARNING: this bundle requires macOS {worst}, not the intended {target}.",
        file=sys.stderr,
    )
    print(
        "    WARNING: the bundled third-party dylibs were built for a newer macOS, so",
        file=sys.stderr,
    )
    print(
        "    WARNING: the app will abort at launch on older systems (missing symbols).",
        file=sys.stderr,
    )
    print(
        "    WARNING: build the bundle on the oldest macOS you intend to support.",
        file=sys.stderr,
    )
print(worst)
PY
)"

echo "==> Writing Info.plist (LSMinimumSystemVersion $ACTUAL_MIN)"
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
	<string>$ACTUAL_MIN</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSPrincipalClass</key>
	<string>NSApplication</string>
</dict>
</plist>
PLIST

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
