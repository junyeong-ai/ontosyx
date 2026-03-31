# Ontosyx Web

Next.js 16 frontend for the Ontosyx knowledge graph platform.

## Tech Stack

- Next.js 16.1, React 19, TypeScript 5
- Tailwind CSS 4, Zustand 5
- streamdown 2.5 (AI-optimized streaming markdown) + @streamdown/code (Shiki)
- Motion 12 (animations), Recharts 3 (charts), xyflow 12 (ontology canvas)

## Run

```bash
pnpm install
pnpm dev          # http://localhost:3100
```

Or use the project-level dev manager:

```bash
../scripts/dev.sh fe start    # Start frontend
../scripts/dev.sh fe log      # Tail logs
../scripts/dev.sh fe restart  # Restart
```

## Authentication

- **JWT mode**: OIDC login (Google, Microsoft, etc.) via server-side proxy
- **API key mode**: Server injects `OX_API_KEY` env var via BFF proxy
- Workspace isolation: all API calls scoped by workspace context

## Environment

| Variable | Default | Description |
|----------|---------|-------------|
| `ONTOSYX_API_URL` | `http://localhost:3001/api` | Backend API URL |
| `OX_API_KEY` | — | API key for dev mode (injected by proxy) |
| `PORT` | `3100` | Dev server port |
