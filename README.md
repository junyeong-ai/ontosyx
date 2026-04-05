# Ontosyx

Knowledge Graph Lifecycle Platform — design ontologies from source data, translate natural language into graph queries, and analyze knowledge graphs through an AI agent.

## Architecture

```
Source DB / CSV / Code Repo
  → ox-source (schema introspection)
  → ox-brain (LLM ontology design via branchforge)
  → OntologyIR
  → ox-brain (LLM query translation)
  → QueryIR
  → ox-compiler (IR → Cypher)
  → ox-runtime (Neo4j execution)
  → Results + Visualization
```

### Crates

| Crate | Description |
|-------|-------------|
| `ox-core` | Domain types — OntologyIR, QueryIR, LoadPlan, SourceSchema, OntologyCommand |
| `ox-brain` | LLM orchestration via branchforge — ClientPool, ModelResolver, prompt caching |
| `ox-compiler` | IR → Cypher compiler, export (Python, TypeScript, GraphQL, OWL, SHACL, Mermaid) |
| `ox-runtime` | Neo4j driver with retry, sandbox isolation, workspace-scoped queries |
| `ox-store` | PostgreSQL persistence — store traits, RLS workspace isolation, migrations |
| `ox-source` | Data source introspection — PostgreSQL, MySQL, MongoDB, CSV |
| `ox-memory` | Semantic memory — ONNX embedding + pgvector search |
| `ox-agent` | AI agent with domain tools built on branchforge |
| `ox-api` | Axum HTTP server — REST API, SSE streaming, OIDC auth |

### Frontend

Next.js 16, React 19, Tailwind CSS 4, Zustand 5, streamdown (AI-optimized streaming markdown).

## Quick Start

### Prerequisites

- Rust 1.94+
- Node.js 22+ / pnpm 10+
- Docker (Neo4j + PostgreSQL with pgvector)

### Setup

```bash
# Start everything (Docker + backend + frontend)
./scripts/dev.sh start

# Or individually:
./scripts/dev.sh docker up     # Infrastructure only
./scripts/dev.sh be start      # Backend on :3101
./scripts/dev.sh fe start      # Frontend on :3100
```

### Service Management

```bash
./scripts/dev.sh              # Status dashboard + health check
./scripts/dev.sh status       # Service status
./scripts/dev.sh health       # API health + component checks
./scripts/dev.sh restart      # Restart BE + FE
./scripts/dev.sh be log       # Tail backend logs
./scripts/dev.sh fe log       # Tail frontend logs
./scripts/dev.sh stop         # Stop BE + FE
./scripts/dev.sh clean        # Full reset (volumes + rebuild)
```

### URLs

| Service | URL |
|---------|-----|
| Frontend | http://localhost:3100 |
| Backend API | http://localhost:3101/api/health |
| Swagger UI | http://localhost:3101/swagger-ui/ |
| Neo4j Browser | http://localhost:7474 |

## API Endpoints

### Chat (SSE Agent Streaming)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/chat/stream` | Agent chat with SSE streaming, tool calls, model override |

### Design Projects

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/projects` | Create project + analyze source |
| POST | `/api/projects/{id}/design` | Generate ontology via LLM |
| POST | `/api/projects/{id}/refine` | Refine with graph profile |
| POST | `/api/projects/{id}/edit` | Surgical ontology edits |
| POST | `/api/projects/{id}/complete` | Promote to saved ontology |
| POST | `/api/projects/{id}/deploy-schema` | Deploy schema to Neo4j |

### Model Management

| Method | Path | Description |
|--------|------|-------------|
| GET/POST | `/api/models/configs` | List / create model configs |
| PATCH/DELETE | `/api/models/configs/{id}` | Update / delete model config |
| GET/POST | `/api/models/routing-rules` | List / create routing rules |
| PATCH/DELETE | `/api/models/routing-rules/{id}` | Update / delete routing rule |
| POST | `/api/models/test` | Test model connectivity |

### Query

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/query/history` | List query executions |
| POST | `/api/query/raw` | Direct Cypher execution |
| POST | `/api/query/from-ir` | Execute from QueryIR JSON |
| PATCH | `/api/query/history/{id}/feedback` | Submit accuracy feedback (toggleable) |

### Additional Endpoints

Workspaces, ontology export/import, dashboards, recipes, reports, sessions,
ACL policies, audit log, usage metering, data lineage, quality rules,
approval workflows, admin prompt management. See Swagger UI for full list.

## Authentication

Three auth methods (configurable in `ontosyx.toml`):

- **JWT** — OIDC providers (Google, Microsoft, Okta, Auth0, Keycloak)
- **API Key** — for programmatic/CI access (`X-API-Key` header)
- **Workspace isolation** — Row-Level Security scopes all data per workspace

## Configuration

Layered precedence: defaults → `ontosyx.toml` → `OX_*` env vars.

Key sections: `[server]`, `[graph]`, `[postgres]`, `[llm]`, `[fast_llm]`,
`[embedding]`, `[auth]`, `[timeouts]`, `[retention]`, `[mcp]`.

Model configuration is DB-backed (runtime-changeable via `/api/models/configs`).
TOML values seed the DB on first boot.

## License

MIT
