// Workbench extensions — explicit registration list.
//
// Drop a new extension into `src/extensions/<my-extension>.ts`,
// import it here, and add it to the `EXTENSIONS` array. The boot
// pass at startup invokes each extension's `install(api)` exactly
// once. There is no auto-discovery on purpose — the explicit list
// keeps the production bundle static-analyzable and tree-shakeable,
// and makes it trivial to disable an extension by removing one line.
//
// The registration order matters: extensions can depend on facets /
// modes registered earlier in the array. The default-shipping
// inspector facets and workbench modes are registered before
// `installExtensions()` runs, so an extension that hooks into
// `before: "definition"` etc. can rely on the defaults being there.

import type { WorkbenchExtension } from "./types";

export const EXTENSIONS: WorkbenchExtension[] = [
  // No first-party extensions yet — the manifest is open for
  // workspace-specific plugins to register custom modes / facets
  // without forking the registry call sites.
];

export type { WorkbenchExtension, WorkbenchExtensionAPI } from "./types";
export {
  installExtensions,
  listInstalledExtensionIds,
  _uninstallAllExtensionsForTests,
} from "./install";
