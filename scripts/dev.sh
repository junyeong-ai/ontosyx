#!/usr/bin/env bash
# ============================================================================
# Ontosyx Dev Manager — start/stop/restart frontend & backend services
#
# Usage:
#   ./scripts/dev.sh                  Interactive status overview
#   ./scripts/dev.sh start            Start all services (docker + be + fe)
#   ./scripts/dev.sh stop             Stop be + fe (leaves docker running)
#   ./scripts/dev.sh restart          Restart be + fe
#   ./scripts/dev.sh status           Show service status
#   ./scripts/dev.sh be [start|stop|restart|log]
#   ./scripts/dev.sh fe [start|stop|restart|log]
#   ./scripts/dev.sh docker [up|down|reset|status]
#   ./scripts/dev.sh log [be|fe]      Tail service logs
#   ./scripts/dev.sh health           Run health checks
#   ./scripts/dev.sh target [status|prune|clean]
#   ./scripts/dev.sh clean            Full reset (docker volumes + rebuild)
# ============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
WEB_DIR="$ROOT_DIR/web"

# Wire git pre-commit hooks on first invocation. The script is
# idempotent — once `core.hooksPath` points at `.githooks/`, this
# is a silent no-op on every subsequent boot.
if [ -f "$SCRIPT_DIR/setup-hooks.sh" ]; then
    bash "$SCRIPT_DIR/setup-hooks.sh"
fi

# ── Ports ───────────────────────────────────────────────────────
BE_PORT="${OX_BE_PORT:-3101}"
FE_PORT="${OX_FE_PORT:-3100}"
PG_PORT=5436
NEO4J_BOLT=7687
NEO4J_HTTP=7474
CARGO_PROFILE="${OX_CARGO_PROFILE:-dev}"
CARGO_FEATURES="${OX_CARGO_FEATURES-source-all}"

# ── Logs ────────────────────────────────────────────────────────
BE_LOG="/tmp/ontosyx-be.log"
FE_LOG="/tmp/ontosyx-fe.log"
BE_SESSION="ontosyx-be-${BE_PORT}"
FE_SESSION="ontosyx-fe-${FE_PORT}"

# ── Colors (2026 modern palette — subtle, high-contrast) ───────
R=$'\033[38;5;203m'    # Red (error/stopped)
G=$'\033[38;5;114m'    # Green (running/ok)
Y=$'\033[38;5;221m'    # Yellow (warning/action)
B=$'\033[38;5;75m'     # Blue (info)
C=$'\033[38;5;73m'     # Cyan (label)
D=$'\033[38;5;242m'    # Dim (secondary)
M=$'\033[38;5;183m'    # Magenta (header accent)
W=$'\033[1m'           # Bold white
N=$'\033[0m'           # Reset

# ── Icons ───────────────────────────────────────────────────────
OK="${G}●${N}"
NO="${R}○${N}"
ARROW="${D}→${N}"
WARN="${Y}▲${N}"

# ── Utility ─────────────────────────────────────────────────────
_pid_on_port() { lsof -ti :"$1" 2>/dev/null | head -1; }
_is_running()  { [ -n "$(_pid_on_port "$1")" ]; }

_kill_port() {
  local port=$1
  local pids
  pids=$(lsof -ti :"$port" 2>/dev/null || true)
  if [ -n "$pids" ]; then
    echo "$pids" | xargs kill -9 2>/dev/null || true
    sleep 1
  fi
}

_stop_screen_session() {
  local session=$1
  if command -v screen >/dev/null 2>&1; then
    screen -S "$session" -X quit 2>/dev/null || true
  fi
}

_screen_session_exists() {
  local session=$1
  command -v screen >/dev/null 2>&1 && screen -ls 2>/dev/null | grep -q "[.]${session}[[:space:]]"
}

_start_screen_session() {
  local session=$1
  local command=$2
  if command -v screen >/dev/null 2>&1; then
    _stop_screen_session "$session"
    screen -dmS "$session" zsh -lc "$command"
    return 0
  fi
  return 1
}

_be_process_alive() {
  _screen_session_exists "$BE_SESSION" || pgrep -f "$(_be_binary)" >/dev/null 2>&1
}

_profile_dir() {
  case "$CARGO_PROFILE" in
    dev) echo "debug" ;;
    *) echo "$CARGO_PROFILE" ;;
  esac
}

_cargo() {
  if command -v mise >/dev/null 2>&1 && [ -f "$ROOT_DIR/mise.toml" ]; then
    (cd "$ROOT_DIR" && mise exec -- cargo "$@")
    return
  fi
  cargo "$@"
}

_cargo_build_be() {
  local feature_args=()
  if [ -n "$CARGO_FEATURES" ]; then
    feature_args=(--features "$CARGO_FEATURES")
  fi
  _cargo build --profile "$CARGO_PROFILE" "${feature_args[@]}" --bin ontosyx --manifest-path "$ROOT_DIR/Cargo.toml" 2>&1 \
    | tail -3 | sed 's/^/  /'
}

_be_binary() {
  echo "$ROOT_DIR/target/$(_profile_dir)/ontosyx"
}

_du_path() {
  local path=$1
  if [ -e "$path" ]; then
    du -sh "$path" 2>/dev/null | awk '{print $1}'
  else
    printf "-"
  fi
}

_wait_ready() {
  local port="$1" label="$2" max="${3:-60}" url="${4:-}"
  [ -z "$url" ] && url="http://localhost:${port}"
  for i in $(seq 1 "$max"); do
    if curl -s "$url" -o /dev/null --max-time 2 2>/dev/null; then
      return 0
    fi
    printf "\r  ${D}waiting for ${label}... ${i}/${max}s${N}"
    sleep 1
  done
  printf "\r  ${R}timeout waiting for ${label} (${max}s)${N}\n"
  return 1
}

_wait_backend_ready() {
  local max="${1:-90}" url="http://localhost:${BE_PORT}/api/health"
  for i in $(seq 1 "$max"); do
    if curl -s "$url" -o /dev/null --max-time 2 2>/dev/null; then
      return 0
    fi
    if ! _be_process_alive; then
      printf "\r  ${R}backend exited before it became ready${N}\n"
      tail -50 "$BE_LOG" | sed 's/^/  /'
      return 1
    fi
    printf "\r  ${D}waiting for backend... ${i}/${max}s${N}"
    sleep 1
  done
  printf "\r  ${R}timeout waiting for backend (${max}s)${N}\n"
  return 1
}

_badge() {
  if _is_running "$1"; then
    local pid=$(_pid_on_port "$1")
    printf "${OK} ${G}running${N} ${D}pid:${pid}${N}"
  else
    printf "${NO} ${D}stopped${N}"
  fi
}

_docker_badge() {
  local svc=$1
  local state
  state=$(docker compose -f "$ROOT_DIR/docker-compose.yml" ps --format '{{.Status}}' "$svc" 2>/dev/null | head -1)
  if echo "$state" | grep -qi "up"; then
    if echo "$state" | grep -qi "healthy"; then
      printf "${OK} ${G}healthy${N}"
    else
      printf "${WARN} ${Y}starting${N}"
    fi
  else
    printf "${NO} ${D}stopped${N}"
  fi
}

# ── Header ──────────────────────────────────────────────────────
_header() {
  echo ""
  echo "  ${M}╔══════════════════════════════════════════╗${N}"
  echo "  ${M}║${N}  ${W}Ontosyx${N} ${D}Dev Manager${N}                     ${M}║${N}"
  echo "  ${M}╚══════════════════════════════════════════╝${N}"
  echo ""
}

# ── Status Display ──────────────────────────────────────────────
_status() {
  _header

  echo "  ${C}SERVICES${N}"
  echo "  ${D}──────────────────────────────────────────${N}"
  printf "  %-14s %s\n" "Backend"  "$(_badge $BE_PORT) ${D}:${BE_PORT}${N}"
  printf "  %-14s %s\n" "Frontend" "$(_badge $FE_PORT) ${D}:${FE_PORT}${N}"
  echo ""

  echo "  ${C}INFRASTRUCTURE${N}"
  echo "  ${D}──────────────────────────────────────────${N}"
  printf "  %-14s %s\n" "PostgreSQL" "$(_docker_badge postgres) ${D}:${PG_PORT}${N}"
  printf "  %-14s %s\n" "Neo4j"      "$(_docker_badge neo4j) ${D}:${NEO4J_BOLT}${N}"
  echo ""

  echo "  ${C}LOGS${N}"
  echo "  ${D}──────────────────────────────────────────${N}"
  echo "  ${D}BE${N} $ARROW ${D}${BE_LOG}${N}"
  echo "  ${D}FE${N} $ARROW ${D}${FE_LOG}${N}"
  echo ""
}

# ── Docker ──────────────────────────────────────────────────────
_docker_up() {
  echo "  ${B}Starting Docker services...${N}"
  docker compose -f "$ROOT_DIR/docker-compose.yml" up -d 2>&1 | sed 's/^/  /'
  echo "  ${G}Docker services started${N}"
}

_docker_down() {
  echo "  ${Y}Stopping Docker services...${N}"
  docker compose -f "$ROOT_DIR/docker-compose.yml" down 2>&1 | sed 's/^/  /'
  echo "  ${G}Docker services stopped${N}"
}

_docker_reset() {
  echo "  ${R}Resetting Docker volumes (all data will be lost)...${N}"
  docker compose -f "$ROOT_DIR/docker-compose.yml" down -v 2>&1 | sed 's/^/  /'
  echo "  ${G}Docker volumes removed${N}"
}

_docker_status() {
  docker compose -f "$ROOT_DIR/docker-compose.yml" ps 2>&1 | sed 's/^/  /'
}

# ── Backend ─────────────────────────────────────────────────────
_start_be() {
  if _is_running $BE_PORT; then
    echo "  ${WARN} Backend already running on :${BE_PORT}"
    return 0
  fi

  echo "  ${B}Building backend...${N}"
  _cargo_build_be

  echo "  ${B}Starting backend on :${BE_PORT}...${N}"
  : > "$BE_LOG"
  if ! _start_screen_session "$BE_SESSION" "cd '$ROOT_DIR' && exec '$(_be_binary)' > '$BE_LOG' 2>&1"; then
    cd "$ROOT_DIR"
    nohup "$(_be_binary)" > "$BE_LOG" 2>&1 &
    cd - > /dev/null
  fi

  if _wait_backend_ready 90; then
    echo ""
    echo "  ${OK} ${G}Backend ready${N} ${D}:${BE_PORT}${N}"
  else
    echo ""
    echo "  ${NO} ${R}Backend failed to start${N}"
    echo "  ${D}Check logs: tail -50 ${BE_LOG}${N}"
    return 1
  fi
}

_stop_be() {
  _stop_screen_session "$BE_SESSION"
  if ! _is_running $BE_PORT; then
    echo "  ${D}Backend not running${N}"
    return 0
  fi
  echo "  ${Y}Stopping backend...${N}"
  _kill_port $BE_PORT
  echo "  ${G}Backend stopped${N}"
}

# ── Frontend ────────────────────────────────────────────────────
_start_fe() {
  if _is_running $FE_PORT; then
    echo "  ${WARN} Frontend already running on :${FE_PORT}"
    return 0
  fi

  # Clean stale lock
  rm -f "$WEB_DIR/.next/dev/lock" 2>/dev/null || true

  echo "  ${B}Starting frontend on :${FE_PORT}...${N}"
  : > "$FE_LOG"
  if ! _start_screen_session "$FE_SESSION" "cd '$WEB_DIR' && PORT='$FE_PORT' exec pnpm dev > '$FE_LOG' 2>&1"; then
    cd "$WEB_DIR"
    PORT=$FE_PORT nohup pnpm dev > "$FE_LOG" 2>&1 &
    cd - > /dev/null
  fi

  if _wait_ready "$FE_PORT" "frontend" 30; then
    echo ""
    echo "  ${OK} ${G}Frontend ready${N} ${D}:${FE_PORT}${N}"
  else
    echo ""
    echo "  ${NO} ${R}Frontend failed to start${N}"
    echo "  ${D}Check logs: tail -50 ${FE_LOG}${N}"
    return 1
  fi
}

_stop_fe() {
  _stop_screen_session "$FE_SESSION"
  if ! _is_running $FE_PORT; then
    echo "  ${D}Frontend not running${N}"
    return 0
  fi
  echo "  ${Y}Stopping frontend...${N}"
  _kill_port $FE_PORT
  # Also kill any orphaned next-server processes
  pkill -f "next dev.*ontosyx" 2>/dev/null || true
  echo "  ${G}Frontend stopped${N}"
}

# ── Health Check ────────────────────────────────────────────────
_jq_or_die() {
  if ! command -v jq >/dev/null 2>&1; then
    echo "  ${R}jq not found — install with \`brew install jq\` (macOS) or your package manager${N}" >&2
    return 1
  fi
}

_dev_cred() {
  local name=$1
  local value="${!name:-}"
  if [ -n "$value" ]; then
    printf "%s" "$value"
    return 0
  fi

  local creds="$(_creds_file)"
  if [ -f "$creds" ]; then
    sed -nE "s/^export ${name}=\"(.*)\"$/\\1/p" "$creds" | head -1
  fi
}

_health() {
  echo ""
  echo "  ${C}HEALTH CHECK${N}"
  echo "  ${D}──────────────────────────────────────────${N}"

  _jq_or_die || { echo ""; return 1; }

  # Backend API
  if _is_running $BE_PORT; then
    local resp
    resp=$(curl -s "http://localhost:${BE_PORT}/api/healthz" --max-time 5 2>/dev/null || echo '{}')

    local status pg_ok graph_ok llm_status llm_provider llm_model graph_backend
    status=$(echo "$resp"        | jq -r '.status // "?"')
    pg_ok=$(echo "$resp"         | jq -r '.components.postgres // "?"')
    graph_ok=$(echo "$resp"      | jq -r '.components.graph // "?"')
    graph_backend=$(echo "$resp" | jq -r '.components.graph_backend // ""')
    llm_status=$(echo "$resp"    | jq -r '.components.llm.status // "configured"')
    llm_provider=$(echo "$resp"  | jq -r '.components.llm.provider // "?"')
    llm_model=$(echo "$resp"     | jq -r '.components.llm.model // "?"')

    case "$status" in
      ok)        echo "  ${OK} ${G}API health: ok${N}" ;;
      degraded)  echo "  ${WARN} ${Y}API health: degraded${N}" ;;
      *)         echo "  ${NO} ${R}API health: ${status}${N}" ;;
    esac

    _component_line() {
      local label=$1 value=$2
      if [ "$value" = "ok" ]; then
        printf "  ${D}  %-9s${N} ${G}%s${N}\n" "${label}:" "$value"
      elif [ -z "$value" ] || [ "$value" = "?" ]; then
        printf "  ${D}  %-9s %s${N}\n" "${label}:" "${value:-?}"
      else
        printf "  ${D}  %-9s${N} ${R}%s${N}\n" "${label}:" "$value"
      fi
    }
    _component_line "postgres" "$pg_ok"
    _component_line "graph"    "$graph_ok"
    if [ -n "$graph_backend" ] && [ "$graph_backend" != "none" ]; then
      printf "  ${D}  %-9s %s${N}\n" "backend:" "$graph_backend"
    fi
    printf "  ${D}  %-9s %s/%s (%s)${N}\n" "llm:" "$llm_provider" "$llm_model" "$llm_status"
  else
    echo "  ${NO} ${D}Backend not running${N}"
  fi

  # Frontend
  if _is_running $FE_PORT; then
    if curl -s "http://localhost:${FE_PORT}" -o /dev/null --max-time 2 2>/dev/null; then
      echo "  ${OK} ${G}Frontend: responding${N}"
    else
      echo "  ${WARN} ${Y}Frontend: port open but not responding${N}"
    fi
  else
    echo "  ${NO} ${D}Frontend not running${N}"
  fi

  if _is_running $BE_PORT; then
    local api_key workspace_id
    api_key="$(_dev_cred OX_API_KEY)"
    workspace_id="$(_dev_cred OX_WORKSPACE_ID)"

    if [ -z "$api_key" ]; then
      printf "  ${D}  %-9s %s${N}\n" "configs:" "skipped (run ./scripts/dev.sh seed)"
    else
      local headers=("-H" "x-api-key: ${api_key}")
      if [ -n "$workspace_id" ]; then
        headers+=("-H" "x-workspace-id: ${workspace_id}")
      fi

      local config_resp config_body config_status model_count
      config_resp=$(curl -sS -w $'\n%{http_code}' "http://localhost:${BE_PORT}/api/models/configs" \
        "${headers[@]}" --max-time 5 2>/dev/null || printf '\n000')
      config_body="${config_resp%$'\n'*}"
      config_status="${config_resp##*$'\n'}"
      if [[ "$config_status" =~ ^2 ]]; then
        model_count=$(echo "$config_body" | jq -r '(.data // .) | if type == "array" then length else 0 end' 2>/dev/null || echo "0")
        printf "  ${D}  %-9s %s${N}\n" "configs:" "$model_count"
      else
        printf "  ${D}  %-9s ${R}%s${N}\n" "configs:" "HTTP ${config_status}"
      fi

      if [ -n "${llm_provider:-}" ] && [ "$llm_provider" != "?" ] && [ -n "${llm_model:-}" ] && [ "$llm_model" != "?" ]; then
        local api_key_env model_body probe_resp probe_body probe_status probe_ok probe_message
        case "$llm_provider" in
          anthropic)
            if [ -n "${OX_LLM__API_KEY:-}" ]; then
              api_key_env="OX_LLM__API_KEY"
            else
              api_key_env="ANTHROPIC_API_KEY"
            fi
            ;;
          openai) api_key_env="OPENAI_API_KEY" ;;
          *) api_key_env="" ;;
        esac
        model_body=$(jq -cn \
          --arg provider "$llm_provider" \
          --arg model "$llm_model" \
          --arg api_key_env "$api_key_env" \
          '{
            provider: $provider,
            model_id: $model,
            api_key_env: (if $api_key_env == "" then null else $api_key_env end),
            region: null,
            base_url: null
          }')
        probe_resp=$(curl -sS -w $'\n%{http_code}' -X POST "http://localhost:${BE_PORT}/api/models/test" \
          "${headers[@]}" -H "content-type: application/json" --data-binary "$model_body" --max-time 20 2>/dev/null || printf '\n000')
        probe_body="${probe_resp%$'\n'*}"
        probe_status="${probe_resp##*$'\n'}"
        if [[ "$probe_status" =~ ^2 ]]; then
          probe_ok=$(echo "$probe_body" | jq -r '.data.ok // false' 2>/dev/null || echo "false")
          probe_message=$(echo "$probe_body" | jq -r '.data.message // ""' 2>/dev/null || echo "")
          if [ "$probe_ok" = "true" ]; then
            printf "  ${D}  %-9s ${G}%s${N}\n" "llm-probe:" "ok"
          else
            printf "  ${D}  %-9s ${R}%s${N}\n" "llm-probe:" "${probe_message:-failed}"
          fi
        else
          printf "  ${D}  %-9s ${R}%s${N}\n" "llm-probe:" "HTTP ${probe_status}"
        fi
      fi
    fi
  fi

  echo ""
}

# ── Log Tailing ─────────────────────────────────────────────────
_log_be() { echo "  ${D}Tailing backend log (Ctrl+C to stop)${N}"; tail -f "$BE_LOG"; }
_log_fe() { echo "  ${D}Tailing frontend log (Ctrl+C to stop)${N}"; tail -f "$FE_LOG"; }

# ── Command Router ──────────────────────────────────────────────
cmd_start() {
  _header
  _docker_up
  echo ""
  _start_be
  echo ""
  _start_fe
  echo ""
  echo "  ${W}All services started${N}"
  echo "  ${D}Backend${N}  $ARROW ${B}http://localhost:${BE_PORT}/swagger-ui/${N}"
  echo "  ${D}Frontend${N} $ARROW ${B}http://localhost:${FE_PORT}${N}"
  echo ""
}

cmd_stop() {
  _header
  _stop_fe
  _stop_be
  echo ""
}

cmd_restart() {
  _header
  _stop_fe
  _stop_be
  echo ""
  _start_be
  echo ""
  _start_fe
  echo ""
  echo "  ${W}Restart complete${N}"
  echo ""
}

cmd_be() {
  local sub="${1:-status}"
  case "$sub" in
    start)   _start_be ;;
    stop)    _stop_be ;;
    restart) _stop_be; echo ""; _start_be ;;
    log)     _log_be ;;
    status)  printf "  Backend  %s\n" "$(_badge $BE_PORT)" ;;
    *)       echo "  ${R}Unknown: be ${sub}${N}. Use: start|stop|restart|log|status" ;;
  esac
}

cmd_fe() {
  local sub="${1:-status}"
  case "$sub" in
    start)   _start_fe ;;
    stop)    _stop_fe ;;
    restart) _stop_fe; echo ""; _start_fe ;;
    log)     _log_fe ;;
    status)  printf "  Frontend %s\n" "$(_badge $FE_PORT)" ;;
    *)       echo "  ${R}Unknown: fe ${sub}${N}. Use: start|stop|restart|log|status" ;;
  esac
}

cmd_docker() {
  local sub="${1:-status}"
  case "$sub" in
    up)     _docker_up ;;
    down)   _docker_down ;;
    reset)  _docker_reset ;;
    status) _docker_status ;;
    *)      echo "  ${R}Unknown: docker ${sub}${N}. Use: up|down|reset|status" ;;
  esac
}

cmd_log() {
  local target="${1:-be}"
  case "$target" in
    be) _log_be ;;
    fe) _log_fe ;;
    *)  echo "  ${R}Unknown: log ${target}${N}. Use: be|fe" ;;
  esac
}

cmd_clean() {
  _header
  echo "  ${R}Full environment reset${N}"
  echo ""
  _stop_fe
  _stop_be
  echo ""
  _docker_reset
  echo ""
  # Purge cached dev credentials too — the DB volumes are gone, so
  # the hash of this key no longer matches any api_keys row. Leaving
  # it cached would mislead the next `seed` into "reusing cached
  # key" when the key is actually dead.
  if [ -f "$(_creds_file)" ]; then
    rm -f "$(_creds_file)"
    echo "  ${D}Removed $(_creds_file)${N}"
  fi
  # Drop the dev workspace id so a later `seed` rewrites it cleanly;
  # preserve any other settings the user added to .env.local.
  local fe_env="$WEB_DIR/.env.local"
  if [ -f "$fe_env" ]; then
    grep -v "^NEXT_PUBLIC_OX_DEV_WORKSPACE_ID=" "$fe_env" > "$fe_env.tmp" || true
    mv "$fe_env.tmp" "$fe_env"
  fi
  echo ""
  echo "  ${B}Rebuilding backend...${N}"
  _cargo_build_be
  echo ""
  echo "  ${G}Clean complete. Run ${W}./scripts/dev.sh seed${N}${G} to re-seed credentials.${N}"
  echo ""
}

cmd_target() {
  local sub="${1:-status}"
  case "$sub" in
    status)
      echo ""
      echo "  ${C}TARGET CACHE${N}"
      echo "  ${D}──────────────────────────────────────────${N}"
      printf "  %-18s %s\n" "target" "$(_du_path "$ROOT_DIR/target")"
      printf "  %-18s %s\n" "debug" "$(_du_path "$ROOT_DIR/target/debug")"
      printf "  %-18s %s\n" "debug/deps" "$(_du_path "$ROOT_DIR/target/debug/deps")"
      printf "  %-18s %s\n" "debug/incremental" "$(_du_path "$ROOT_DIR/target/debug/incremental")"
      printf "  %-18s %s\n" "dev-fast" "$(_du_path "$ROOT_DIR/target/dev-fast")"
      printf "  %-18s %s\n" "release" "$(_du_path "$ROOT_DIR/target/release")"
      echo ""
      ;;
    prune)
      _header
      echo "  ${Y}Pruning reproducible dev build artifacts...${N}"
      _stop_fe
      _stop_be
      rm -rf "$ROOT_DIR/target/debug" "$ROOT_DIR/target/dev-fast"
      echo "  ${G}Removed target/debug and target/dev-fast${N}"
      echo "  ${D}Release/container artifacts are preserved.${N}"
      echo ""
      ;;
    clean)
      _header
      echo "  ${Y}Removing all Cargo build artifacts...${N}"
      _stop_fe
      _stop_be
      _cargo clean --manifest-path "$ROOT_DIR/Cargo.toml"
      echo "  ${G}Cargo target directory cleaned${N}"
      echo ""
      ;;
    *)
      echo "  ${R}Unknown: target ${sub}${N}. Use: status|prune|clean"
      ;;
  esac
}

# Path is a two-file contract — the FE proxy reads the same location
# in `web/src/lib/server/api-proxy.ts` (DEV_CREDS_PATH).
_creds_file() { echo "/tmp/ontosyx-dev-creds"; }

# Mint (or reuse) a dev bootstrap credential — the same auth path
# prod uses, just with a well-known local-only key. Writes
# `/tmp/ontosyx-dev-creds` as a sourceable shell file so follow-on
# commands (`. ./scripts/dev.sh seed >&2; source $(./scripts/dev.sh creds-file)`)
# can pick up `OX_API_KEY` + `OX_WORKSPACE_ID` without copy-paste.
cmd_seed() {
  _header
  local creds="$(_creds_file)"

  # 1. Mint a fresh dev key if one isn't cached already.
  local key
  if [ -f "$creds" ]; then
    # shellcheck disable=SC1090
    source "$creds"
    key="${OX_API_KEY:-}"
  fi
  if [ -z "${key:-}" ]; then
    key="dev-$(openssl rand -hex 16)"
    echo "  ${B}Minted new dev API key${N}"
  else
    echo "  ${D}Reusing cached dev API key${N}"
  fi

  # 2. Launch the backend with OX_AUTH__BOOTSTRAP_KEY set. On an
  #    empty `api_keys` table this seeds the key; on a populated
  #    table the bootstrap path is a no-op and the cached key must
  #    still match.
  if _is_running $BE_PORT; then
    echo "  ${Y}Stopping backend to re-seed with bootstrap key...${N}"
    _stop_be
  fi
  export OX_AUTH__BOOTSTRAP_KEY="$key"
  echo "  ${B}Starting backend with bootstrap key...${N}"
  _start_be

  # 3. Resolve the default workspace (auto-seeded during boot) and
  #    cache the credentials.
  local ws_id
  ws_id=$(curl -s -H "x-api-key: $key" \
    "http://localhost:${BE_PORT}/api/workspaces" \
    | python3 -c "
import json, sys
try:
    data = json.load(sys.stdin).get('data', [])
    items = data if isinstance(data, list) else data.get('items', [])
    if items:
        print(items[0]['id'])
except Exception:
    pass")

  if [ -z "$ws_id" ]; then
    echo "  ${R}Default workspace not returned — check backend log${N}"
    return 1
  fi

  # 4. Write cached creds + FE env.
  cat > "$creds" <<EOF
# Generated by \`./scripts/dev.sh seed\`. Source this file before
# running curl against the local backend:
#   source $(basename "$creds")
#   curl -H "x-api-key: \$OX_API_KEY" -H "x-workspace-id: \$OX_WORKSPACE_ID" ...
export OX_API_KEY="$key"
export OX_WORKSPACE_ID="$ws_id"
EOF
  chmod 600 "$creds"

  # The API key lives only in $creds — the FE proxy reads it fresh
  # per request so a re-seed propagates without restarting Next.js.
  # Workspace_id is browser-visible (identifier, not secret) and is
  # consumed by `getWorkspaceId()` as a default; it ships through the
  # NEXT_PUBLIC_* env, which still requires `dev.sh fe restart`.
  local fe_env="$WEB_DIR/.env.local"
  if [ -f "$fe_env" ]; then
    grep -v "^NEXT_PUBLIC_OX_DEV_WORKSPACE_ID=" "$fe_env" > "$fe_env.tmp" || true
    mv "$fe_env.tmp" "$fe_env"
  fi
  cat >> "$fe_env" <<EOF
NEXT_PUBLIC_OX_DEV_WORKSPACE_ID=$ws_id
EOF

  echo ""
  echo "  ${G}Dev credentials ready${N}"
  echo "  ${D}────────────────────────────────────────────${N}"
  echo "  ${C}api key       :${N} $key"
  echo "  ${C}workspace_id  :${N} $ws_id"
  echo "  ${C}cached at     :${N} $creds"
  echo "  ${C}frontend env  :${N} $fe_env  ${D}(NEXT_PUBLIC_OX_DEV_WORKSPACE_ID)${N}"
  echo ""
  echo "  ${D}curl example:${N}"
  echo "    source $creds"
  echo "    curl -H \"x-api-key: \$OX_API_KEY\" -H \"x-workspace-id: \$OX_WORKSPACE_ID\" \\"
  echo "         http://localhost:${BE_PORT}/api/ontology"
  echo ""
  echo "  ${Y}Restart the frontend if NEXT_PUBLIC_OX_DEV_WORKSPACE_ID changed:${N}"
  echo "    ./scripts/dev.sh fe restart  ${D}(API key flows through ${creds} live)${N}"
  echo ""
}

cmd_help() {
  _header
  echo "  ${C}COMMANDS${N}"
  echo "  ${D}──────────────────────────────────────────${N}"
  echo "  ${W}start${N}            Start all (docker + be + fe)"
  echo "  ${W}stop${N}             Stop be + fe"
  echo "  ${W}restart${N}          Restart be + fe"
  echo "  ${W}status${N}           Show service status"
  echo "  ${W}health${N}           Run health checks"
  echo "  ${W}seed${N}             Mint dev API key + default workspace, write creds"
  echo "  ${W}target${N} ${D}[status|prune|clean]${N}"
  echo ""
  echo "  ${W}be${N} ${D}[start|stop|restart|log]${N}"
  echo "  ${W}fe${N} ${D}[start|stop|restart|log]${N}"
  echo "  ${W}docker${N} ${D}[up|down|reset|status]${N}"
  echo "  ${W}log${N} ${D}[be|fe]${N}       Tail logs"
  echo ""
  echo "  ${W}clean${N}            Full reset (stop all + remove volumes)"
  echo ""
  echo "  ${C}ENVIRONMENT${N}"
  echo "  ${D}──────────────────────────────────────────${N}"
  echo "  ${D}OX_BE_PORT${N}  Backend port  ${D}(default: ${BE_PORT})${N}"
  echo "  ${D}OX_FE_PORT${N}  Frontend port ${D}(default: ${FE_PORT})${N}"
  echo "  ${D}OX_CARGO_PROFILE${N} Backend profile ${D}(default: ${CARGO_PROFILE}; use dev-fast explicitly)${N}"
  echo "  ${D}OX_CARGO_FEATURES${N} Backend features ${D}(active: ${CARGO_FEATURES:-none}; default source-all, set empty for lean build)${N}"
  echo ""
}

# ── Main ────────────────────────────────────────────────────────
main() {
  local cmd="${1:-}"
  shift 2>/dev/null || true

  case "$cmd" in
    start)   cmd_start ;;
    stop)    cmd_stop ;;
    restart) cmd_restart ;;
    status)  _status ;;
    health)  _health ;;
    target)  cmd_target "$@" ;;
    be)      cmd_be "$@" ;;
    fe)      cmd_fe "$@" ;;
    docker)  cmd_docker "$@" ;;
    log)     cmd_log "$@" ;;
    clean)   cmd_clean ;;
    seed)    cmd_seed ;;
    help|-h|--help) cmd_help ;;
    "")      _status; _health ;;
    *)       echo "  ${R}Unknown command: ${cmd}${N}"; echo ""; cmd_help ;;
  esac
}

main "$@"
