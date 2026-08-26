#!/usr/bin/env bash
set -euo pipefail

binary="$(realpath "${1:?Linux release binary is required}")"
tray_host="${2:-unavailable}"
release_validation="${3:-false}"
case "${tray_host}" in
  available | unavailable) ;;
  *)
    echo "Linux tray host must be available or unavailable: ${tray_host}" >&2
    exit 1
    ;;
esac
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
export USAGEDECK_SMOKE_TRAY_HOST="${tray_host}"

xvfb-run -a dbus-run-session -- bash -euo pipefail -c '
  runner_temp="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
  export HOME
  HOME="$(mktemp -d "${runner_temp}/usagedeck-x11-home.XXXXXX")"
  export XDG_CONFIG_HOME="${HOME}/xdg"
  export XDG_STATE_HOME="${HOME}/state"
  export XDG_CURRENT_DESKTOP="ubuntu:GNOME"
  export XDG_SESSION_TYPE="x11"
  mkdir -p "${XDG_CONFIG_HOME}" "${XDG_STATE_HOME}"
  stdio_log="${runner_temp}/usagedeck-x11-app-${RANDOM}.log"
  runtime_log="${XDG_STATE_HOME}/usagedeck/logs/UsageDeck.log"
  wm_log="${runner_temp}/usagedeck-x11-openbox-${RANDOM}.log"
  watcher_log="${runner_temp}/usagedeck-x11-watcher-${RANDOM}.log"
  watcher_pid=""

  if test "${USAGEDECK_SMOKE_TRAY_HOST}" = available; then
    command -v dbus-test-tool >/dev/null || {
      echo "dbus-test-tool is required for the Linux tray-host smoke test." >&2
      exit 1
    }
    unset USAGEDECK_LINUX_TRAY_HOST
    dbus-test-tool echo --session --name=org.kde.StatusNotifierWatcher \
      >"${watcher_log}" 2>&1 &
    watcher_pid=$!
    watcher_ready=false
    for _ in $(seq 1 20); do
      if dbus-send --session --dest=org.freedesktop.DBus --type=method_call --print-reply \
        /org/freedesktop/DBus org.freedesktop.DBus.NameHasOwner \
        string:org.kde.StatusNotifierWatcher 2>/dev/null | grep -Fq "boolean true"; then
        watcher_ready=true
        break
      fi
      if ! kill -0 "${watcher_pid}" 2>/dev/null; then
        cat "${watcher_log}" >&2 || true
        exit 1
      fi
      sleep 1
    done
    if test "${watcher_ready}" != true; then
      cat "${watcher_log}" >&2 || true
      echo "The StatusNotifier watcher did not acquire its D-Bus name." >&2
      exit 1
    fi
  else
    export USAGEDECK_LINUX_TRAY_HOST="unavailable"
  fi

  openbox >"${wm_log}" 2>&1 &
  wm_pid=$!
  app_pid=""
  cleanup() {
    if test -n "${app_pid}"; then
      kill "${app_pid}" 2>/dev/null || true
      wait "${app_pid}" 2>/dev/null || true
    fi
    kill "${wm_pid}" 2>/dev/null || true
    wait "${wm_pid}" 2>/dev/null || true
    if test -n "${watcher_pid}"; then
      kill "${watcher_pid}" 2>/dev/null || true
      wait "${watcher_pid}" 2>/dev/null || true
    fi
  }
  trap cleanup EXIT

  "${USAGEDECK_SMOKE_BINARY}" >"${stdio_log}" 2>&1 &
  app_pid=$!
  ready=false
  for _ in $(seq 1 30); do
    if ! kill -0 "${app_pid}" 2>/dev/null; then
      cat "${stdio_log}" >&2 || true
      cat "${runtime_log}" >&2 || true
      exit 1
    fi
    if test "${USAGEDECK_SMOKE_TRAY_HOST}" = available; then
      if test -f "${runtime_log}" \
        && grep -Fq "desktop integration detected (tray=true)" "${runtime_log}" \
        && grep -Fq "system tray integration ready" "${runtime_log}" \
        && grep -Fq "UsageDeck startup completed" "${runtime_log}"; then
        ready=true
        break
      fi
    elif test -f "${runtime_log}" \
      && grep -Fq "desktop integration detected (tray=false)" "${runtime_log}" \
      && grep -Fq "UsageDeck startup completed" "${runtime_log}" \
      && xdotool search --onlyvisible --limit 1 --pid "${app_pid}" --name "^UsageDeck$" >/dev/null 2>&1; then
      ready=true
      break
    fi
    sleep 1
  done
  if test "${ready}" != true; then
    cat "${stdio_log}" >&2 || true
    cat "${runtime_log}" >&2 || true
    cat "${wm_log}" >&2 || true
    cat "${watcher_log}" >&2 || true
    echo "UsageDeck did not enter the expected ${USAGEDECK_SMOKE_TRAY_HOST} tray-host mode." >&2
    exit 1
  fi
  if test "${USAGEDECK_SMOKE_TRAY_HOST}" = available; then
    kill "${watcher_pid}"
    wait "${watcher_pid}" 2>/dev/null || true
    watcher_pid=""
    fallback_ready=false
    for _ in $(seq 1 30); do
      if ! kill -0 "${app_pid}" 2>/dev/null; then
        cat "${stdio_log}" >&2 || true
        cat "${runtime_log}" >&2 || true
        exit 1
      fi
      if grep -Fq "system tray became unavailable; using standalone window" "${runtime_log}" \
        && xdotool search --onlyvisible --limit 1 --pid "${app_pid}" --name "^UsageDeck$" >/dev/null 2>&1; then
        fallback_ready=true
        break
      fi
      sleep 1
    done
    if test "${fallback_ready}" != true; then
      cat "${stdio_log}" >&2 || true
      cat "${runtime_log}" >&2 || true
      echo "UsageDeck did not expose its standalone window after the tray host stopped." >&2
      exit 1
    fi
  elif grep -Fq "system tray integration ready" "${runtime_log}"; then
    echo "UsageDeck created a tray while the tray host was unavailable." >&2
    exit 1
  fi

  close_attempted=false
  close_requested=false
  for _ in $(seq 1 10); do
    if ! kill -0 "${app_pid}" 2>/dev/null; then
      if test "${close_attempted}" = true; then
        close_requested=true
        break
      fi
      cat "${stdio_log}" >&2 || true
      cat "${runtime_log}" >&2 || true
      echo "UsageDeck exited before its standalone window was closed." >&2
      exit 1
    fi
    window_id="$(xdotool search --onlyvisible --limit 1 --pid "${app_pid}" --name "^UsageDeck$" 2>/dev/null || true)"
    if test -n "${window_id}"; then
      close_attempted=true
      if xdotool windowclose "${window_id}" 2>/dev/null; then
        close_requested=true
        break
      fi
    fi
    sleep 1
  done
  if test "${close_requested}" != true; then
    cat "${stdio_log}" >&2 || true
    cat "${runtime_log}" >&2 || true
    echo "UsageDeck did not keep a visible standalone window available for closing." >&2
    exit 1
  fi
  exited=false
  for _ in $(seq 1 20); do
    if ! kill -0 "${app_pid}" 2>/dev/null; then
      wait "${app_pid}" 2>/dev/null || true
      app_pid=""
      exited=true
      break
    fi
    sleep 1
  done
  if test "${exited}" != true; then
    cat "${stdio_log}" >&2 || true
    cat "${runtime_log}" >&2 || true
    echo "UsageDeck did not exit when its standalone window was closed." >&2
    exit 1
  fi
'
