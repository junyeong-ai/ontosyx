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
#   ./scripts/dev.sh clean            Full reset (docker volumes + rebuild)
# ============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
WEB_DIR="$ROOT_DIR/web"

# ── Ports ───────────────────────────────────────────────────────
BE_PORT="${OX_BE_PORT:-3001}"
FE_PORT="${OX_FE_PORT:-3100}"
PG_PORT=5433
NEO4J_BOLT=7687
NEO4J_HTTP=7474

# ── Logs ────────────────────────────────────────────────────────
BE_LOG="/tmp/ontosyx-be.log"
FE_LOG="/tmp/ontosyx-fe.log"

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
  cargo build --bin ontosyx --manifest-path "$ROOT_DIR/Cargo.toml" 2>&1 | tail -3 | sed 's/^/  /'

  echo "  ${B}Starting backend on :${BE_PORT}...${N}"
  cd "$ROOT_DIR"
  nohup cargo run --bin ontosyx > "$BE_LOG" 2>&1 &
  cd - > /dev/null

  if _wait_ready "$BE_PORT" "backend" 90 "http://localhost:${BE_PORT}/api/health"; then
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
  cd "$WEB_DIR"
  PORT=$FE_PORT nohup pnpm dev > "$FE_LOG" 2>&1 &
  cd - > /dev/null

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
_health() {
  echo ""
  echo "  ${C}HEALTH CHECK${N}"
  echo "  ${D}──────────────────────────────────────────${N}"

  # Backend API
  if _is_running $BE_PORT; then
    local resp
    resp=$(curl -s "http://localhost:${BE_PORT}/api/health" 2>/dev/null || echo '{}')
    local status
    status=$(echo "$resp" | python3 -c "import sys,json; print(json.load(sys.stdin).get('status','?'))" 2>/dev/null || echo "error")
    if [ "$status" = "ok" ]; then
      echo "  ${OK} ${G}API health: ok${N}"
    elif [ "$status" = "degraded" ]; then
      echo "  ${WARN} ${Y}API health: degraded${N}"
    else
      echo "  ${NO} ${R}API health: ${status}${N}"
    fi

    # Component details
    local pg_ok neo4j_ok llm_model
    pg_ok=$(echo "$resp" | python3 -c "import sys,json; print(json.load(sys.stdin).get('components',{}).get('postgres','?'))" 2>/dev/null || echo "?")
    neo4j_ok=$(echo "$resp" | python3 -c "import sys,json; print(json.load(sys.stdin).get('components',{}).get('neo4j','?'))" 2>/dev/null || echo "?")
    llm_model=$(echo "$resp" | python3 -c "import sys,json; print(json.load(sys.stdin).get('components',{}).get('llm',{}).get('model','?'))" 2>/dev/null || echo "?")

    echo "  ${D}  postgres: ${pg_ok}${N}"
    echo "  ${D}  neo4j:    ${neo4j_ok}${N}"
    echo "  ${D}  llm:      ${llm_model}${N}"
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

  # Model configs
  if _is_running $BE_PORT; then
    local model_count
    model_count=$(curl -s "http://localhost:${BE_PORT}/api/models/configs" \
      -H "X-API-Key: dev-api-key-ontosyx" 2>/dev/null | \
      python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "0")
    echo "  ${D}  model configs: ${model_count}${N}"
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
  echo "  ${B}Rebuilding backend...${N}"
  cargo build --bin ontosyx --manifest-path "$ROOT_DIR/Cargo.toml" 2>&1 | tail -3 | sed 's/^/  /'
  echo ""
  echo "  ${G}Clean complete. Run ${W}./scripts/dev.sh start${N}${G} to bring everything up.${N}"
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
    be)      cmd_be "$@" ;;
    fe)      cmd_fe "$@" ;;
    docker)  cmd_docker "$@" ;;
    log)     cmd_log "$@" ;;
    clean)   cmd_clean ;;
    help|-h|--help) cmd_help ;;
    "")      _status; _health ;;
    *)       echo "  ${R}Unknown command: ${cmd}${N}"; echo ""; cmd_help ;;
  esac
}

main "$@"
