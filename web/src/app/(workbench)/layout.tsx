"use client";

import { useEffect } from "react";
import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";
import { Sidebar } from "@/components/layout/sidebar";
import { Header } from "@/components/layout/header";
import { MobileNavRoot } from "@/components/layout/mobile-nav-root";
import { WorkspaceNotificationProbe } from "@/components/layout/workspace-notification-probe";
import { PageTransition } from "@/components/motion/page-transition";
import { GlobalCommandSource } from "@/components/layout/global-command-source";
import { KeyboardShortcutsDialog } from "@/components/ui/keyboard-shortcuts-dialog";
import { ErrorBoundary } from "@/components/ui/error-boundary";
import { QualityBanner } from "@/components/quality/quality-banner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { WelcomeModal } from "@/components/onboarding/welcome-modal";
import { SessionExpiredOverlay } from "@/components/collab/session-expired-overlay";
import { AuthGuard } from "@/components/auth/auth-guard";
import { useHydrated } from "@/lib/store/use-hydrated";
import { useNavigationShortcuts } from "@/hooks/use-navigation-shortcuts";
import { useAppStore } from "@/lib/store";
import { useShortcut } from "@/lib/shortcuts";
import {
  fetchWsToken,
  useCollab,
  useCollabRoom,
  useNetworkAwareness,
  useVisibilityAwareness,
} from "@/lib/collab";
import { CollaborationErrorToaster } from "@/components/collab/collaboration-error-toaster";
import { selectStateActiveOntologyDraft } from "@/lib/store/selectors";

// `NEXT_PUBLIC_WS_URL` lets ops point the workbench at a different
// host than the HTTP API (e.g. a dedicated WS-fanout pod). The dev
// default mirrors `dev.sh`'s backend port.
const COLLAB_WS_URL =
  process.env.NEXT_PUBLIC_WS_URL ?? "ws://localhost:3101/ws/collab";

/**
 * Shared shell for every workspace mode. Each mode owns its own
 * route segment under this group; the shell renders Sidebar +
 * Header + chrome and each segment's `page.tsx` fills the `<main>`.
 *
 * SSR / hydration contract: the provider tree and outer flex shell
 * render unconditionally so the React tree shape is identical across
 * server render, first client render, and post-hydration re-render.
 * Only the inner chrome differs between skeleton and live state, and
 * that swap happens inside the stable wrapper — returning two
 * structurally different roots would put `loading.tsx`'s implicit
 * Suspense at a different tree position on server vs. client.
 */
export default function WorkbenchLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const tSkip = useTranslations("chrome.skipLinks");
  const pathname = usePathname();
  const hydrated = useHydrated();
  const workspaceReady = useAppStore((s) => s.workspaceReady);
  const workspaceId = useAppStore((s) => s.workspaceId);
  const initWorkspace = useAppStore((s) => s.initWorkspace);
  // Inspector skip-link is workbench-scoped — root layout only
  // owns the universal `toMain`/`toSidebar` pair, since the inspector
  // landmark is design-only and depends on whether an ontology is
  // loaded and the inspector panel toggle is open. Mirroring the
  // exact mount conditions here keeps `aria-controls`-style integrity:
  // the skip-link only appears when the target it points to actually
  // exists in the DOM, so axe never fires `skip-link` violations.
  const inspectorOpen = useAppStore((s) => s.isInspectorOpen);
  const hasOntology = useAppStore((s) => !!s.ontology);
  // `startsWith` so any future `/design/*` sub-route inherits the
  // skip-link without a parallel allowlist.
  const showInspectorSkipLink =
    pathname.startsWith("/design") && inspectorOpen && hasOntology;

  // Collaboration WebSocket — single socket per workspace, shared
  // across every workbench mode. The hook tears the socket down
  // automatically when `workspaceId` clears or switches.
  const collabClient = useCollab({
    url: COLLAB_WS_URL,
    workspaceId: workspaceId ?? "",
    getToken: fetchWsToken,
  });
  useNetworkAwareness(collabClient);
  useVisibilityAwareness();
  // Auto-join the active project's collab room. Switching
  // projects re-joins; clearing the active project leaves.
  const activeOntologyDraftForCollab = useAppStore(selectStateActiveOntologyDraft);
  useCollabRoom(activeOntologyDraftForCollab?.id);
  useNavigationShortcuts();

  // Initialize workspace after Zustand hydration — same bootstrap the
  // old `page.tsx` performed.
  useEffect(() => {
    if (hydrated && !workspaceReady) {
      initWorkspace();
    }
  }, [hydrated, workspaceReady, initWorkspace]);

  useShortcut({
    id: "workbench.toggleSidebar",
    keys: ["[", "Mod+b"],
    group: "keyboardShortcuts.sections.global",
    description: "keyboardShortcuts.shortcuts.toggleSidebar",
    handler: (e) => {
      e.preventDefault();
      useAppStore.getState().toggleSidebarMode();
    },
  });

  return (
    <AuthGuard>
    <ErrorBoundary>
      <TooltipProvider>
        {/* React tree shape stays identical across SSR / first client
            render / post-hydration so React's reconciliation has no
            structural mismatch and the skip-link target (`#main`) is
            an interactive landmark the entire time — never `aria-hidden`,
            never duplicated. Pre-hydration `aria-busy` lets assistive
            tech announce "loading" without relocating the focus target. */}
        {/* Workbench-scoped skip links, placed *before* the sidebar so
            keyboard users hit them as the first focusables inside this
            layout — without this position the skips would land mid-tree
            after the chrome and stop being "skips". The root layout
            owns only `#main`; surface-specific targets live with the
            layout that actually renders the landmark, so axe never
            sees a skip link pointing at a missing element. */}
        <nav aria-label={tSkip("labelWorkbench")} className="contents">
          <a
            href="#sidebar"
            className="sr-only focus:not-sr-only focus:absolute focus:start-4 focus:top-4 focus:z-skip-link focus:rounded-md focus:bg-brand-solid focus:px-4 focus:py-2 focus:text-sm focus:font-medium focus:text-foreground-onbrand focus:outline-none focus:ring-2 focus:ring-brand-foreground/40 focus:ring-offset-2"
          >
            {tSkip("toSidebar")}
          </a>
          {showInspectorSkipLink && (
            <a
              href="#inspector"
              className="sr-only focus:not-sr-only focus:absolute focus:start-4 focus:top-4 focus:z-skip-link focus:rounded-md focus:bg-brand-solid focus:px-4 focus:py-2 focus:text-sm focus:font-medium focus:text-foreground-onbrand focus:outline-none focus:ring-2 focus:ring-brand-foreground/40 focus:ring-offset-2"
            >
              {tSkip("toInspector")}
            </a>
          )}
        </nav>
        <div className="flex h-dvh overflow-hidden" aria-busy={!hydrated}>
          <MobileNavRoot>
            {hydrated ? <Sidebar /> : <SidebarRailSkeleton />}
          </MobileNavRoot>
          <div className="flex flex-1 flex-col overflow-hidden">
            {hydrated ? <Header /> : <HeaderSkeleton />}
            {hydrated && workspaceReady && <QualityBanner />}
            {/* `tabIndex={0}` makes the skip-link target focusable —
                otherwise `<main>` is bypassed when keyboard users
                activate `#main`, and axe flags the skip link as
                pointing nowhere. The `focus-visible:` ring keeps the
                indicator scoped to keyboard activation. */}
            <main
              id="main"
              tabIndex={0}
              className="flex-1 overflow-hidden outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/50 focus-visible:ring-inset"
            >
              {hydrated && (
                <div className="h-full overflow-hidden">
                  <PageTransition motionKey={pathname}>{children}</PageTransition>
                </div>
              )}
            </main>
          </div>
        </div>
        <KeyboardShortcutsDialog />
        <GlobalCommandSource />
        <CollaborationErrorToaster />
        <WelcomeModal />
        <SessionExpiredOverlay />
        {hydrated && workspaceReady && <WorkspaceNotificationProbe />}
      </TooltipProvider>
    </ErrorBoundary>
    </AuthGuard>
  );
}

// Rail skeleton matches the live `<Sidebar>`'s rail width so the
// post-hydration shape doesn't shift. No interactive children — the
// real Sidebar replaces this once the store hydrates.
function SidebarRailSkeleton() {
  return (
    <div
      className="w-rail shrink-0 border-e border-divider bg-surface-raised"
      aria-hidden="true"
    />
  );
}

// Header skeleton mirrors the live header's height so the canvas /
// page body don't reflow at hydration time.
function HeaderSkeleton() {
  return (
    <div className="h-10 shrink-0 border-b border-divider" aria-hidden />
  );
}
