#!/usr/bin/env bash
set -euo pipefail

binary="$(realpath "${1:?Linux release binary is required}")"
release_validation="${2:-false}"
case "${release_validation}" in
  true | false) ;;
  *)
    echo "Linux release validation must be true or false: ${release_validation}" >&2
    exit 1
    ;;
esac
test -x "${binary}"
export USAGEDECK_SMOKE_BINARY="${binary}"
export USAGEDECK_SMOKE_RELEASE_VALIDATION="${release_validation}"

dbus-run-session -- bash -euo pipefail -c '
  runner_temp="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
  export HOME
  HOME="$(mktemp -d "${runner_temp}/usagedeck-wayland-home.XXXXXX")"
  export XDG_CONFIG_HOME="${HOME}/xdg"
  export XDG_STATE_HOME="${HOME}/state"
  export XDG_CURRENT_DESKTOP="KDE"
  export XDG_RUNTIME_DIR
  XDG_RUNTIME_DIR="$(mktemp -d "${runner_temp}/usagedeck-wayland-runtime.XXXXXX")"
  export XDG_SESSION_TYPE="wayland"
  export USAGEDECK_LINUX_TRAY_HOST="unavailable"
  export GDK_BACKEND="wayland"
  export WAYLAND_DISPLAY="usagedeck-wayland"
  mkdir -p "${XDG_CONFIG_HOME}" "${XDG_STATE_HOME}"
  chmod 700 "${XDG_RUNTIME_DIR}"
  stdio_log="${runner_temp}/usagedeck-wayland-app-${RANDOM}.log"
  runtime_log="${XDG_STATE_HOME}/usagedeck/logs/UsageDeck.log"
  weston_log="${runner_temp}/usagedeck-weston-${RANDOM}.log"
  weston --backend=headless-backend.so --socket="${WAYLAND_DISPLAY}" --idle-time=0 \
    --log="${weston_log}" &
  weston_pid=$!
  app_pid=""
  cleanup() {
    if test -n "${app_pid}"; then
      kill "${app_pid}" 2>/dev/null || true
      wait "${app_pid}" 2>/dev/null || true
    fi
    kill "${weston_pid}" 2>/dev/null || true
    wait "${weston_pid}" 2>/dev/null || true
  }
  trap cleanup EXIT
  for _ in $(seq 1 20); do
    if test -S "${XDG_RUNTIME_DIR}/${WAYLAND_DISPLAY}"; then
      break
    fi
    if ! kill -0 "${weston_pid}" 2>/dev/null; then
      cat "${weston_log}"
      exit 1
    fi
    sleep 1
  done
  if ! test -S "${XDG_RUNTIME_DIR}/${WAYLAND_DISPLAY}"; then
    cat "${weston_log}"
    exit 1
  fi
  "${USAGEDECK_SMOKE_BINARY}" >"${stdio_log}" 2>&1 &
  app_pid=$!
  ready=false
  for _ in $(seq 1 30); do
    if ! kill -0 "${app_pid}" 2>/dev/null; then
      cat "${stdio_log}" >&2 || true
      cat "${runtime_log}" >&2 || true
      exit 1
    fi
    if test -f "${runtime_log}" \
      && grep -Fq "desktop integration detected (tray=false)" "${runtime_log}" \
      && grep -Fq "UsageDeck startup completed" "${runtime_log}"; then
      ready=true
      break
    fi
    sleep 1
  done
  if test "${ready}" != true; then
    cat "${stdio_log}" >&2 || true
    cat "${runtime_log}" >&2 || true
    cat "${weston_log}" >&2 || true
    echo "UsageDeck did not report a ready Wayland fallback before the startup deadline." >&2
    exit 1
  fi
  if grep -Fq "system tray integration ready" "${runtime_log}"; then
    echo "UsageDeck created a tray while the Wayland tray host was unavailable." >&2
    exit 1
  fi
'
