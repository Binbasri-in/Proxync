#!/usr/bin/env bash
# Proxync Universal One-Line Installer (Linux & macOS)
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/Inilax/Proxync/main/scripts/install.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/Inilax/Proxync/main/scripts/install.sh | bash -s -- --gui
set -euo pipefail

INSTALL_GUI=false
for arg in "$@"; do
    case "$arg" in
        --gui) INSTALL_GUI=true ;;
    esac
done

REPO="Inilax/Proxync"
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$ARCH" in
    x86_64|amd64) ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

echo -e "\033[1;36m==> Installing Proxync for ${OS}-${ARCH}...\033[0m"

# Target installation directory for CLI
if [ -w "/usr/local/bin" ]; then
    BIN_DIR="/usr/local/bin"
else
    BIN_DIR="${HOME}/.local/bin"
    mkdir -p "${BIN_DIR}"
fi

LATEST_RELEASE="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null || true)"
VERSION="$(echo "${LATEST_RELEASE}" | grep -o '"tag_name": *"[^"]*"' | cut -d'"' -f4 || true)"
if [ -z "${VERSION}" ]; then
    VERSION="v0.2.4"
fi

CLI_NAME="proxync-${OS}-${ARCH}"
CLI_URL="https://github.com/${REPO}/releases/download/${VERSION}/${CLI_NAME}.tar.gz"

echo "==> Fetching Proxync CLI (${VERSION})..."
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

if curl -fsSL "${CLI_URL}" -o "${TMP_DIR}/proxync.tar.gz" 2>/dev/null; then
    tar -xzf "${TMP_DIR}/proxync.tar.gz" -C "${TMP_DIR}"
    chmod +x "${TMP_DIR}/proxync"
    mv "${TMP_DIR}/proxync" "${BIN_DIR}/proxync"
else
    # Fallback to local cargo build if running in cloned repo
    if [ -f "packages/cli/Cargo.toml" ]; then
        echo "Release asset not found. Building locally via cargo..."
        cargo build --release --manifest-path="packages/cli/Cargo.toml"
        cp "packages/cli/target/release/proxync" "${BIN_DIR}/proxync"
    else
        echo "Error: Could not download release binary from ${CLI_URL}" >&2
        exit 1
    fi
fi

echo -e "\033[1;32m[OK] Proxync CLI installed to ${BIN_DIR}/proxync\033[0m"

# Install Desktop GUI if requested
if [ "${INSTALL_GUI}" = true ]; then
    echo -e "\033[1;36m==> Installing Proxync Desktop GUI...\033[0m"
    if [ "${OS}" = "darwin" ]; then
        DMG_NAME="Proxync_${VERSION#v}_${ARCH}.dmg"
        DMG_URL="https://github.com/${REPO}/releases/download/${VERSION}/${DMG_NAME}"
        echo "Downloading ${DMG_URL}..."
        curl -fsSL "${DMG_URL}" -o "${TMP_DIR}/${DMG_NAME}" || true
        if [ -f "${TMP_DIR}/${DMG_NAME}" ]; then
            mkdir -p "${TMP_DIR}/mnt"
            hdiutil attach "${TMP_DIR}/${DMG_NAME}" -nobrowse -mountpoint "${TMP_DIR}/mnt"
            cp -R "${TMP_DIR}/mnt/Proxync.app" /Applications/
            hdiutil detach "${TMP_DIR}/mnt"
            echo -e "\033[1;32m[OK] Proxync.app installed to /Applications\033[0m"
        else
            echo "Desktop release asset not available yet. Build locally with: npm run tauri build"
        fi
    elif [ "${OS}" = "linux" ]; then
        APPIMAGE_NAME="proxync_${VERSION#v}_amd64.AppImage"
        APPIMAGE_URL="https://github.com/${REPO}/releases/download/${VERSION}/${APPIMAGE_NAME}"
        echo "Downloading ${APPIMAGE_URL}..."
        curl -fsSL "${APPIMAGE_URL}" -o "${BIN_DIR}/proxync-desktop" || true
        if [ -f "${BIN_DIR}/proxync-desktop" ]; then
            chmod +x "${BIN_DIR}/proxync-desktop"
            echo -e "\033[1;32m[OK] Proxync Desktop installed to ${BIN_DIR}/proxync-desktop\033[0m"
        fi
    fi
fi

# Ensure bin dir in PATH hint
case ":${PATH}:" in
    *:"${BIN_DIR}":*) ;;
    *) echo -e "\n\033[33mNote: Add ${BIN_DIR} to your PATH by adding this to ~/.bashrc or ~/.zshrc:\033[0m\n  export PATH=\"${BIN_DIR}:\$PATH\"" ;;
esac

echo -e "\n\033[1;32mDone!\033[0m Try running: \033[1mproxync --help\033[0m or \033[1mproxync scan\033[0m"
