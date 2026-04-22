import { redirect } from "next/navigation";

/**
 * Root landing — immediately redirects to the default workspace mode.
 *
 * After Phase 2-4 each mode owns its own route (`/design`, `/analyze`,
 * `/explore`, `/dashboard`). The root slash is kept as a friendly entry
 * point so existing bookmarks still land somewhere useful.
 */
export default function RootRedirect() {
  redirect("/design");
}
