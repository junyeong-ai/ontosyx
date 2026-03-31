# Ontosyx

**Semantic Orchestrator** — Knowledge Graph Lifecycle Platform

Ontosyx analyzes source data, asks for user review where semantics are ambiguous, designs an ontology, and translates natural language into graph queries through an IR-based pipeline.

## Architecture

```
[Source DB / CSV / JSON]
  → source analysis
  → user review for ambiguity / PII
  → ox-brain (LLM ontology design)
  → [OntologyIR]
  → ox-brain (LLM query translation)
  → [QueryIR]
  → ox-compiler
  → [Cypher]
  → ox-runtime
  → [Neo4j]
```

### Crates

| Crate | Description |
|-------|-------------|
| `ox-core` | Shared IR types (OntologyIR, QueryIR, LoadPlan, WidgetSpec, SourceSchema) |
| `ox-brain` | LLM orchestration (Anthropic, OpenAI, Ollama, Bedrock) |
| `ox-compiler` | IR → target query compilation (Cypher) |
| `ox-runtime` | Graph database drivers (Neo4j) |
| `ox-store` | Persistence layer (PostgreSQL) |
| `ox-source` | Data source introspection (PostgreSQL, CSV, JSON, Text) |
| `ox-api` | HTTP API server (Axum) |

### Frontend

Next.js 16 with React 19, Tailwind CSS 4, Zustand.

## Quick Start

### Prerequisites

- Rust 1.94+
- Node.js 22+ / pnpm 10+
- Docker (for Neo4j, PostgreSQL)

### Setup

```bash
# Start infrastructure
docker compose up -d

# Backend
cp .env.example .env
# Edit .env / OX_* env vars with your LLM credentials
cargo run

# Frontend
cd web
pnpm install
pnpm dev
```

### API Endpoints

#### Chat (single-turn NL query)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/chat` | NL → graph query pipeline (JSON response) |
| POST | `/api/chat/stream` | SSE streaming chat |

#### Query

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/query/history` | List query executions (cursor-paginated) |
| GET | `/api/query/history/{id}` | Get a single query execution |
| POST | `/api/query/raw` | Direct Cypher query execution (write-protected) |

#### Pinboard

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/pins` | Create a pin from a query execution |
| GET | `/api/pins` | List pins (cursor-paginated) |
| DELETE | `/api/pins/{id}` | Delete a pin |

#### Design Projects (ontology lifecycle)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/projects` | Create project + analyze source |
| GET | `/api/projects` | List projects (cursor-paginated) |
| GET | `/api/projects/{id}` | Get project details |
| DELETE | `/api/projects/{id}` | Delete project |
| PATCH | `/api/projects/{id}/decisions` | Update design options (revision CAS) |
| POST | `/api/projects/{id}/design` | Generate ontology via LLM |
| POST | `/api/projects/{id}/reanalyze` | Re-analyze source data |
| POST | `/api/projects/{id}/refine` | Refine ontology with graph profile |
| POST | `/api/projects/{id}/complete` | Promote to saved ontology (quality gate) |

#### Data Loading

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/load` | Generate load plan |
| POST | `/api/load/execute` | Execute load plan |
| GET | `/api/prompts` | List prompt templates |

#### System

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/health` | Health check |

## User Scoping

All user-scoped endpoints require an `X-Principal-Id` header.

- This is a minimal isolation layer, not a full authentication system.
- Query executions and pins are scoped so only the same principal id can read them.
- The bundled web app generates and persists a stable principal id in browser storage automatically.

## Runtime Requirements

The API server can start without Neo4j for design-only work, but query endpoints require a connected graph runtime.

- `/api/chat`
- `/api/chat/stream`
- `/api/query/raw`

## Design Project Workflow

1. Create a project with `POST /api/projects` (includes source analysis).
2. Review the analysis report and update design options with `PATCH /api/projects/{id}/decisions`.
3. Generate an ontology with `POST /api/projects/{id}/design`.
4. Optionally refine with `POST /api/projects/{id}/refine`.
5. Complete and promote with `POST /api/projects/{id}/complete`.

For PostgreSQL sources, ontology design is blocked when unresolved PII decisions or ambiguous columns remain.

## Configuration

Config is loaded in layers: defaults → `ontosyx.toml` → `OX_*` env vars.

See `ontosyx.toml` for all options and `.env.example` for sensitive values.

## License

MIT
