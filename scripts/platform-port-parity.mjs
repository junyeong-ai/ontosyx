// platform-port-parity — keep the local backend port contract explicit.
//
// The Next BFF proxy, Rust server default, dev scripts, Docker image, and docs
// all need to agree on the same API port. A mismatch presents as noisy 503s in
// the browser even when the backend itself is healthy on a different port.

import fs from "node:fs";
import path from "node:path";

const ROOT = path.resolve(new URL("..", import.meta.url).pathname);
const API_PORT = "3101";

const expectations = [
  {
    file: "ontosyx.toml",
    pattern: new RegExp(`\\bport\\s*=\\s*${API_PORT}\\b`),
    description: "canonical TOML server port",
  },
  {
    file: "crates/ox-api/src/config.rs",
    pattern: new RegExp(`\\.set_default\\("server\\.port",\\s*${API_PORT}_i64\\)\\?`),
    description: "Rust config default server port",
  },
  {
    file: "scripts/dev.sh",
    pattern: new RegExp(`BE_PORT="\\$\\{OX_BE_PORT:-${API_PORT}\\}"`),
    description: "dev backend port default",
  },
  {
    file: "scripts/reset-dev.sh",
    pattern: new RegExp(`ONTOSYX_API_URL:-http://localhost:${API_PORT}/api`),
    description: "reset script API URL default",
  },
  {
    file: "web/src/lib/server/api-proxy.ts",
    pattern: new RegExp(`http://localhost:${API_PORT}/api`),
    description: "Next BFF backend URL default",
  },
  {
    file: "Dockerfile",
    pattern: new RegExp(`\\bEXPOSE\\s+${API_PORT}\\b`),
    description: "container exposed backend port",
  },
  {
    file: "README.md",
    pattern: new RegExp(`localhost:${API_PORT}`),
    description: "root README backend URLs",
  },
  {
    file: "web/README.md",
    pattern: new RegExp(`http://localhost:${API_PORT}/api`),
    description: "web README API URL default",
  },
  {
    file: ".env.example",
    pattern: new RegExp(`OX_SERVER__PORT=${API_PORT}`),
    description: "example environment backend port",
  },
];

const failures = [];

for (const expectation of expectations) {
  const fullPath = path.join(ROOT, expectation.file);
  const text = fs.readFileSync(fullPath, "utf8");
  if (!expectation.pattern.test(text)) {
    failures.push(expectation);
  }
}

if (failures.length > 0) {
  console.error("platform-port-parity: backend port contract drift detected:\n");
  for (const failure of failures) {
    console.error(`  ${failure.file} — missing ${failure.description}`);
  }
  console.error(`\nExpected local backend API port: ${API_PORT}`);
  process.exit(1);
}

console.log(`platform-port-parity: backend API port contract is ${API_PORT}.`);
