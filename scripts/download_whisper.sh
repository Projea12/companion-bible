#!/usr/bin/env bash
# Download the Whisper Medium GGML model and verify its SHA-1 checksum.
#
# Usage:
#   bash scripts/download_whisper.sh
#
# The model is saved to models/whisper/ggml-medium.bin (~1.5 GB).
# Requires: curl, shasum (both available on macOS by default).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODEL_DIR="${REPO_ROOT}/models/whisper"
DEST="${MODEL_DIR}/ggml-medium.bin"
TMP="${DEST}.tmp"
URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin"
EXPECTED_SHA1="fd9727b6e1217c2f614f9b698455c4ffd82463b4"

mkdir -p "${MODEL_DIR}"

# ── If already downloaded, just verify ───────────────────────────────────────
if [[ -f "${DEST}" ]]; then
    echo "Model already present at ${DEST}"
    echo "Verifying checksum..."
    ACTUAL=$(shasum -a 1 "${DEST}" | awk '{print $1}')
    if [[ "${ACTUAL}" == "${EXPECTED_SHA1}" ]]; then
        echo "✓  SHA-1 OK (${EXPECTED_SHA1})"
        exit 0
    else
        echo "✗  Checksum mismatch!"
        echo "   Expected : ${EXPECTED_SHA1}"
        echo "   Got      : ${ACTUAL}"
        echo "Re-downloading..."
        rm -f "${DEST}"
    fi
fi

# ── Download ──────────────────────────────────────────────────────────────────
echo "Downloading Whisper Medium GGML (~1.5 GB)..."
echo "  URL  : ${URL}"
echo "  Dest : ${DEST}"
curl --location --fail --progress-bar --output "${TMP}" "${URL}"

# ── Verify ────────────────────────────────────────────────────────────────────
echo "Verifying checksum..."
ACTUAL=$(shasum -a 1 "${TMP}" | awk '{print $1}')
if [[ "${ACTUAL}" != "${EXPECTED_SHA1}" ]]; then
    echo "✗  Checksum mismatch — download may be corrupt."
    echo "   Expected : ${EXPECTED_SHA1}"
    echo "   Got      : ${ACTUAL}"
    rm -f "${TMP}"
    exit 1
fi

mv "${TMP}" "${DEST}"
echo "✓  Model saved to ${DEST}"
echo ""
echo "Run the load test with:"
echo "  cargo test -p companion-transcription model_load -- --ignored --nocapture"
