#!/usr/bin/env bash
set -euo pipefail

TARGET="${1:-ddddocr.onnx}"

if ! command -v python3 >/dev/null 2>&1; then
  echo "未找到 python3，请先安装 Python 并执行: pip install ddddocr" >&2
  exit 1
fi

MODEL_PATH="$(python3 - <<'PY'
import ddddocr, os
print(os.path.join(os.path.dirname(ddddocr.__file__), "common_old.onnx"))
PY
)"

if [ ! -f "$MODEL_PATH" ]; then
  echo "当前 Python 环境没有 ddddocr 包，请先执行: pip install ddddocr" >&2
  exit 1
fi

cp "$MODEL_PATH" "$TARGET"
echo "已将 ddddocr 模型复制到 $TARGET"
