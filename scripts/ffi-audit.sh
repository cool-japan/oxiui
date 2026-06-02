#!/usr/bin/env bash
set -euo pipefail
RESULT=$(cargo tree --workspace --edges normal 2>/dev/null | grep -E '(freetype-sys|harfbuzz-sys|pango-sys|fontconfig-sys|gtk(-sys|4)|gtk4-sys|qmetaobject|qt_core|qt_gui|qt_widgets|sdl2(-sys)?|ring v|aws-lc-sys|openssl-sys|native-tls)' || true)
if [ -n "$RESULT" ]; then
  echo "FFI AUDIT FAILED — forbidden crates found:"
  echo "$RESULT"
  exit 1
fi
echo "FFI audit: PASS (default closure)"
