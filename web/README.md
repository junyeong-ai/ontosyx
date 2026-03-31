# Ontosyx Web

Next.js frontend for the Ontosyx API.

## Run

```bash
pnpm install
pnpm dev
```

Optional environment variable:

```bash
NEXT_PUBLIC_API_URL=http://localhost:3001/api
```

## Principal Scoping

The frontend generates a stable `X-Principal-Id` value in browser storage and attaches it to all user-scoped API requests automatically.

- This is a minimal isolation mechanism before full authentication is introduced.
- Clearing browser storage creates a new principal id and therefore a new user scope.

