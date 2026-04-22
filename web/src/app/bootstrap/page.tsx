"use client";

// /bootstrap (root) redirects to the first step. Kept as a thin
// page so the layout shell mounts on every /bootstrap/** route
// without a 404 hop.

import { redirect } from "next/navigation";

export default function BootstrapRoot() {
  redirect("/bootstrap/1-pilot");
}
