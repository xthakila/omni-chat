#!/bin/bash
set -e

PREFIX="${1:-$HOME/.local}"
INSTALL_DIR="$PREFIX/lib/omnichat"
BIN_DIR="$PREFIX/bin"
APP_DIR="$PREFIX/share/applications"
ICON_DIR="$PREFIX/share/icons/hicolor/256x256/apps"

echo "OmniChat Installer"
echo "==================="
echo "Install prefix: $PREFIX"
echo ""

# Check if release binaries exist
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
if [ -f "$SCRIPT_DIR/target/release/omnichat" ]; then
    OMNICHAT_BIN="$SCRIPT_DIR/target/release/omnichat"
    HELPER_BIN="$SCRIPT_DIR/target/release/omnichat_helper"
elif [ -f "$SCRIPT_DIR/target/debug/omnichat" ]; then
    echo "Warning: using debug build (larger, slower)"
    OMNICHAT_BIN="$SCRIPT_DIR/target/debug/omnichat"
    HELPER_BIN="$SCRIPT_DIR/target/debug/omnichat_helper"
else
    echo "Error: no binaries found. Run 'cargo build --release' first."
    exit 1
fi

# Check CEF
CEF_DIR="${CEF_PATH:-$HOME/.local/share/cef}"
if [ ! -d "$CEF_DIR" ]; then
    echo "CEF not found at $CEF_DIR"
    echo "Install it with: cargo install export-cef-dir && export-cef-dir --force $CEF_DIR"
    exit 1
fi

echo "1. Installing binaries..."
mkdir -p "$INSTALL_DIR" "$BIN_DIR"
cp "$OMNICHAT_BIN" "$INSTALL_DIR/omnichat"
cp "$HELPER_BIN" "$INSTALL_DIR/omnichat_helper"
strip "$INSTALL_DIR/omnichat" 2>/dev/null || true
strip "$INSTALL_DIR/omnichat_helper" 2>/dev/null || true
chmod +x "$INSTALL_DIR/omnichat" "$INSTALL_DIR/omnichat_helper"

echo "2. Installing recipes..."
if [ -d "$SCRIPT_DIR/recipes" ]; then
    cp -r "$SCRIPT_DIR/recipes" "$INSTALL_DIR/recipes"
fi

echo "3. Creating launcher script..."
cat > "$BIN_DIR/omnichat" << LAUNCHER
#!/bin/bash
export CEF_PATH="$CEF_DIR"
export LD_LIBRARY_PATH="\$CEF_PATH:\$LD_LIBRARY_PATH"
cd "$INSTALL_DIR"
exec "$INSTALL_DIR/omnichat" "\$@"
LAUNCHER
chmod +x "$BIN_DIR/omnichat"

echo "4. Installing desktop entry..."
mkdir -p "$APP_DIR" "$ICON_DIR"

# Generate a simple icon (purple O on dark background)
python3 - "$ICON_DIR/omnichat.png" << 'PYEOF'
import sys, struct, zlib

size = 256
pixels = []
cx, cy, r = size/2, size/2, 100
for y in range(size):
    row = []
    for x in range(size):
        dx, dy = x - cx, y - cy
        d = (dx*dx + dy*dy)**0.5
        if 60 < d < r:
            t = (d - 60) / (r - 60)
            row.extend([int(137 + t*66), int(180 - t*14), int(250 - t*3), 255])
        elif d <= 60:
            row.extend([30, 30, 46, 0])
        else:
            row.extend([30, 30, 46, 255 if d < r + 2 else 0])
    pixels.append(bytes(row))

def make_png(w, h, rows):
    def chunk(ctype, data):
        c = ctype + data
        return struct.pack('>I', len(data)) + c + struct.pack('>I', zlib.crc32(c) & 0xffffffff)
    raw = b''
    for row in rows:
        raw += b'\x00' + row
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 6, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(raw)) + chunk(b'IEND', b'')

with open(sys.argv[1], 'wb') as f:
    f.write(make_png(size, size, pixels))
print(f"Icon written to {sys.argv[1]}")
PYEOF

cat > "$APP_DIR/omnichat.desktop" << DESKTOP
[Desktop Entry]
Name=OmniChat
Comment=Lightweight messaging aggregator
Exec=$BIN_DIR/omnichat
Icon=$ICON_DIR/omnichat.png
Type=Application
Categories=Network;InstantMessaging;Chat;
Keywords=chat;messaging;slack;whatsapp;telegram;discord;
StartupWMClass=omnichat
StartupNotify=true
DESKTOP

# CEF may report different app_ids; create symlinks to cover all cases
for name in chromium cef OmniChat Omnichat; do
    ln -sf "$APP_DIR/omnichat.desktop" "$APP_DIR/$name.desktop" 2>/dev/null
done

# Update desktop database
update-desktop-database "$APP_DIR" 2>/dev/null || true
gtk-update-icon-cache "$PREFIX/share/icons/hicolor" 2>/dev/null || true

echo ""
echo "Done! OmniChat installed to $INSTALL_DIR"
echo ""
echo "Binary sizes:"
ls -lh "$INSTALL_DIR/omnichat" "$INSTALL_DIR/omnichat_helper" 2>/dev/null
echo ""
echo "To run:  omnichat"
echo "To uninstall:  rm -rf $INSTALL_DIR $BIN_DIR/omnichat $APP_DIR/omnichat.desktop $ICON_DIR/omnichat.png"
