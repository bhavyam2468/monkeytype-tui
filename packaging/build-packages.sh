#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-0.1.0}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$ROOT_DIR/dist"
BIN="$ROOT_DIR/target/release/monkeytype-tui"
ICON="$ROOT_DIR/assets/icon.png"
DESKTOP="$ROOT_DIR/packaging/monkeytype-tui.desktop"

if [[ ! -x "$BIN" ]]; then
  echo "Building release binary..."
  (cd "$ROOT_DIR" && cargo build --release)
fi

rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

# 0. Direct standalone binaries (run directly or double-click with auto-terminal launch)
install -m755 "$BIN" "$DIST_DIR/monkeytype-tui-linux-x86_64"
echo "Built: dist/monkeytype-tui-linux-x86_64"

STATIC_BIN="$ROOT_DIR/target/x86_64-unknown-linux-musl/release/monkeytype-tui"
if [[ -x "$STATIC_BIN" ]]; then
  install -m755 "$STATIC_BIN" "$DIST_DIR/monkeytype-tui-linux-x86_64-static"
  echo "Built: dist/monkeytype-tui-linux-x86_64-static"
fi

# Prefer static musl binary inside AppImage and tarball if available for max portability
PORTABLE_BIN="$BIN"
if [[ -x "$STATIC_BIN" ]]; then
  PORTABLE_BIN="$STATIC_BIN"
fi

# 1. Generic Linux tar.gz archive
TAR_STAGE="$DIST_DIR/stage-tar/monkeytype-tui-${VERSION}-x86_64-linux"
mkdir -p "$TAR_STAGE"
cp "$PORTABLE_BIN" "$TAR_STAGE/monkeytype-tui"
cp "$DESKTOP" "$TAR_STAGE/monkeytype-tui.desktop"
cp "$ICON" "$TAR_STAGE/monkeytype-tui.png"
cp "$ROOT_DIR/LICENSE" "$TAR_STAGE/LICENSE"
tar -czf "$DIST_DIR/monkeytype-tui-${VERSION}-x86_64-linux.tar.gz" -C "$DIST_DIR/stage-tar" "monkeytype-tui-${VERSION}-x86_64-linux"
rm -rf "$DIST_DIR/stage-tar"
echo "Built: dist/monkeytype-tui-${VERSION}-x86_64-linux.tar.gz"

# 2. Debian (.deb) package built with standard ar + tar (no dpkg-deb required)
DEB_STAGE="$DIST_DIR/stage-deb"
mkdir -p "$DEB_STAGE/data/usr/bin"
mkdir -p "$DEB_STAGE/data/usr/share/applications"
mkdir -p "$DEB_STAGE/data/usr/share/icons/hicolor/256x256/apps"
mkdir -p "$DEB_STAGE/data/usr/share/doc/monkeytype-tui"
mkdir -p "$DEB_STAGE/control"

install -m755 "$BIN" "$DEB_STAGE/data/usr/bin/monkeytype-tui"
install -m644 "$DESKTOP" "$DEB_STAGE/data/usr/share/applications/monkeytype-tui.desktop"
install -m644 "$ICON" "$DEB_STAGE/data/usr/share/icons/hicolor/256x256/apps/monkeytype-tui.png"
install -m644 "$ROOT_DIR/LICENSE" "$DEB_STAGE/data/usr/share/doc/monkeytype-tui/copyright"

INSTALLED_SIZE="$(du -sk "$DEB_STAGE/data" | awk '{print $1}')"
cat > "$DEB_STAGE/control/control" <<EOF
Package: monkeytype-tui
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: amd64
Installed-Size: ${INSTALLED_SIZE}
Maintainer: bhavyam2468 <119266324+bhavyam2468@users.noreply.github.com>
Homepage: https://github.com/bhavyam2468/monkeytype-tui
Description: Minimalist terminal typing test with cursor trail support and DMS theme sync
 A fast terminal typing test inspired by Monkeytype featuring time, words, and
 quote modes, live WPM and accuracy charts, ASCII animations, native Kitty
 cursor trail support, and live DankMaterialShell (DMS) theme syncing.
EOF

printf "2.0\n" > "$DEB_STAGE/debian-binary"
tar --owner=0 --group=0 -czf "$DEB_STAGE/control.tar.gz" -C "$DEB_STAGE/control" .
tar --owner=0 --group=0 -czf "$DEB_STAGE/data.tar.gz" -C "$DEB_STAGE/data" .
(
  cd "$DEB_STAGE"
  ar rcs "$DIST_DIR/monkeytype-tui_${VERSION}_amd64.deb" debian-binary control.tar.gz data.tar.gz
)
rm -rf "$DEB_STAGE"
echo "Built: dist/monkeytype-tui_${VERSION}_amd64.deb"

# 3. Linux AppImage
APPDIR="$DIST_DIR/MonkeytypeTUI.AppDir"
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/share/icons/hicolor/256x256/apps"
install -m755 "$PORTABLE_BIN" "$APPDIR/usr/bin/monkeytype-tui"
install -m644 "$ICON" "$APPDIR/monkeytype-tui.png"
install -m644 "$ICON" "$APPDIR/usr/share/icons/hicolor/256x256/apps/monkeytype-tui.png"
cat > "$APPDIR/monkeytype-tui.desktop" <<EOF
[Desktop Entry]
Name=Monkeytype
Comment=Minimalist terminal typing test
Exec=monkeytype-tui
Icon=monkeytype-tui
Terminal=true
Type=Application
Categories=Utility;Game;
EOF

cat > "$APPDIR/AppRun" <<'EOF'
#!/usr/bin/env sh
HERE="$(dirname "$(readlink -f "${0}")")"
if [ -t 0 ] && [ -t 1 ]; then
  exec "$HERE/usr/bin/monkeytype-tui" "$@"
fi
for term in "${TERMINAL:-}" kitty ghostty alacritty wezterm foot gnome-terminal konsole xterm; do
  if [ -n "$term" ] && command -v "$term" >/dev/null 2>&1; then
    case "$term" in
      kitty) exec kitty --class monkeytype-tui --title "Monkeytype" "$HERE/usr/bin/monkeytype-tui" "$@" ;;
      ghostty|alacritty|wezterm|foot|xterm) exec "$term" -e "$HERE/usr/bin/monkeytype-tui" "$@" ;;
      gnome-terminal) exec gnome-terminal -- "$HERE/usr/bin/monkeytype-tui" "$@" ;;
      konsole) exec konsole -e "$HERE/usr/bin/monkeytype-tui" "$@" ;;
    esac
  fi
done
exec "$HERE/usr/bin/monkeytype-tui" "$@"
EOF
chmod +x "$APPDIR/AppRun"

APPIMAGETOOL="${APPIMAGETOOL:-}"
if [[ -z "$APPIMAGETOOL" ]]; then
  if command -v appimagetool >/dev/null 2>&1; then
    APPIMAGETOOL="$(command -v appimagetool)"
  else
    APPIMAGETOOL="/tmp/appimagetool-x86_64.AppImage"
    if [[ ! -x "$APPIMAGETOOL" ]]; then
      curl -fsSL -o "$APPIMAGETOOL" \
        "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
      chmod +x "$APPIMAGETOOL"
    fi
  fi
fi

ARCH=x86_64 "$APPIMAGETOOL" --appimage-extract-and-run --no-appstream \
  "$APPDIR" "$DIST_DIR/monkeytype-tui-${VERSION}-x86_64.AppImage"
rm -rf "$APPDIR"
echo "Built: dist/monkeytype-tui-${VERSION}-x86_64.AppImage"

# 4. Arch Linux (.pkg.tar.zst) if makepkg is available
if command -v makepkg >/dev/null 2>&1; then
  (
    cd "$ROOT_DIR/packaging"
    PKGDEST="$DIST_DIR" BUILDDIR="/tmp/monkeytype-tui-makepkg" makepkg -f --noconfirm --nodeps
  )
  rm -rf /tmp/monkeytype-tui-makepkg "$ROOT_DIR/packaging/pkg" "$ROOT_DIR/packaging/src"
  echo "Built Arch package in dist/"
fi

# 5. RPM (.rpm) if rpmbuild is available
if command -v rpmbuild >/dev/null 2>&1; then
  RPM_TOP="/tmp/monkeytype-tui-rpmbuild"
  rm -rf "$RPM_TOP"
  mkdir -p "$RPM_TOP"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
  cat > "$RPM_TOP/SPECS/monkeytype-tui.spec" <<EOF
Name:           monkeytype-tui
Version:        ${VERSION}
Release:        1%{?dist}
Summary:        Minimalist terminal typing test with cursor trail support and DMS theme sync
License:        MIT
URL:            https://github.com/bhavyam2468/monkeytype-tui

%description
A fast, minimalist terminal typing test inspired by Monkeytype featuring time,
words, and quote modes, live WPM charts, ASCII animations, native Kitty cursor
trail support, and live DankMaterialShell (DMS) theme syncing.

%install
install -Dm755 "${BIN}" %{buildroot}/usr/bin/monkeytype-tui
install -Dm644 "${DESKTOP}" %{buildroot}/usr/share/applications/monkeytype-tui.desktop
install -Dm644 "${ICON}" %{buildroot}/usr/share/icons/hicolor/256x256/apps/monkeytype-tui.png
install -Dm644 "${ROOT_DIR}/LICENSE" %{buildroot}/usr/share/licenses/monkeytype-tui/LICENSE

%files
/usr/bin/monkeytype-tui
/usr/share/applications/monkeytype-tui.desktop
/usr/share/icons/hicolor/256x256/apps/monkeytype-tui.png
/usr/share/licenses/monkeytype-tui/LICENSE
EOF
  rpmbuild --define "_topdir ${RPM_TOP}" -bb "$RPM_TOP/SPECS/monkeytype-tui.spec"
  cp "$RPM_TOP"/RPMS/x86_64/*.rpm "$DIST_DIR/"
  rm -rf "$RPM_TOP"
  echo "Built RPM package in dist/"
fi

ls -lh "$DIST_DIR"
