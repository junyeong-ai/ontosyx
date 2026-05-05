// Discriminated union describing the load state of a data-driven page.
//
// The page shells (`WorkbenchPageShell`, `SettingsPageShell`) accept a
// `pageState` prop and adjust chrome behaviour automatically:
//
//   - `loading` / `error`: filter row hidden, counter dimmed
//   - `empty`: filter row hidden (no point filtering nothing), counter
//              shows zero
//   - `filtered-empty`: filter row VISIBLE so the user can clear the
//                       filters that produced an empty result; counter
//                       shows "0 of N total"
//   - `data`: filter row visible, counter shows full count
//
// The body of the page is rendered by the page itself — the shell only
// owns the chrome. Pages typically render `<EmptyState>`, `<ErrorState>`,
// `<Skeleton…>`, or the data view based on `pageState.kind`.

export type PageState =
  | { kind: "loading" }
  | { kind: "error"; onRetry: () => void }
  | { kind: "empty" }
  | { kind: "filtered-empty"; onClearFilters: () => void }
  | { kind: "data" };

/** True when the chrome should expose interactive controls (filters / count). */
export function isInteractive(state: PageState): boolean {
  return state.kind === "data" || state.kind === "filtered-empty";
}
