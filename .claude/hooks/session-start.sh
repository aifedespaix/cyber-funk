#!/usr/bin/env bash
# Prépare une session Claude Code dans le cloud : dépendances système de Bevy/cpal/midir, outils cargo.
# Sans effet en local (on suppose la machine déjà équipée).
set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

need=()
for pkg in libasound2-dev libudev-dev libxkbcommon-dev libwayland-dev pkg-config; do
  dpkg -s "$pkg" >/dev/null 2>&1 || need+=("$pkg")
done

if [ ${#need[@]} -gt 0 ]; then
  SUDO=""
  [ "$(id -u)" -ne 0 ] && SUDO="sudo"
  $SUDO apt-get update -qq >/dev/null
  DEBIAN_FRONTEND=noninteractive $SUDO apt-get install -y -qq "${need[@]}" >/dev/null
fi

if command -v rustup >/dev/null 2>&1; then
  rustup component add rustfmt clippy >/dev/null 2>&1 || true
fi

# Précharge les dépendances pour que la première compilation de l'agent soit plus courte.
if [ -f "${CLAUDE_PROJECT_DIR:-.}/Cargo.toml" ]; then
  (cd "${CLAUDE_PROJECT_DIR:-.}" && cargo fetch --locked >/dev/null 2>&1 || cargo fetch >/dev/null 2>&1 || true)
fi

echo "session cyber-funk prête : dépendances système et toolchain vérifiées"
