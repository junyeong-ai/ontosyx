// source-feature-parity — keep source-adapter feature flags explicit.
//
// `ox-source` owns concrete adapters. `ox-api` owns the deployable server
// binary. Every optional adapter registered behind
// `#[cfg(feature = "...")] registry.register("...")` must be exposed as a
// matching `source-*` feature on `ox-api`, otherwise operators cannot enable
// the adapter from the server crate without knowing lower-layer feature names.

import fs from "node:fs";
import path from "node:path";

const ROOT = path.resolve(new URL("..", import.meta.url).pathname);

function read(rel) {
  return fs.readFileSync(path.join(ROOT, rel), "utf8");
}

const registry = read("crates/ox-source/src/registry.rs");
const oxApiCargo = read("crates/ox-api/Cargo.toml");

const adapterFeatures = [
  ...registry.matchAll(
    /#\[cfg\(feature = "([^"]+)"\)\]\s*registry\.register\("([^"]+)"/g,
  ),
].map((match) => ({ feature: match[1], adapter: match[2] }));

const missing = [];
for (const { feature, adapter } of adapterFeatures) {
  const apiFeature = `source-${adapter}`;
  const expected = `${apiFeature} = ["ox-source/${feature}"]`;
  if (!oxApiCargo.includes(expected)) {
    missing.push({ adapter, feature, expected });
  }
}

const sourceFeatures = adapterFeatures.map(({ adapter }) => `source-${adapter}`).sort();
const sourceAllMatch = /\nsource-all\s*=\s*\[([\s\S]*?)\]/.exec(oxApiCargo);
const sourceAllFeatures = sourceAllMatch
  ? [...sourceAllMatch[1].matchAll(/"([^"]+)"/g)].map((match) => match[1]).sort()
  : [];
const sourceAllMissing = sourceFeatures.filter(
  (feature) => !sourceAllFeatures.includes(feature),
);
const sourceAllStale = sourceAllFeatures.filter(
  (feature) => feature.startsWith("source-") && !sourceFeatures.includes(feature),
);

if (!sourceAllMatch) {
  missing.push({
    adapter: "*",
    feature: "*",
    expected: `source-all = [${sourceFeatures.map((feature) => `"${feature}"`).join(", ")}]`,
  });
}

if (missing.length > 0 || sourceAllMissing.length > 0 || sourceAllStale.length > 0) {
  console.error(
    "source-feature-parity: ox-api source feature surface is incomplete:\n",
  );
  for (const item of missing) {
    console.error(
      `  adapter "${item.adapter}" behind ox-source/${item.feature}: add ${item.expected}`,
    );
  }
  for (const feature of sourceAllMissing) {
    console.error(`  source-all missing "${feature}"`);
  }
  for (const feature of sourceAllStale) {
    console.error(`  source-all contains stale "${feature}"`);
  }
  process.exit(1);
}

console.log(
  `source-feature-parity: ${adapterFeatures.length} optional source adapter feature(s) exposed by ox-api and source-all.`,
);
