"use client";

import Image from "next/image";
import { cn } from "@/lib/cn";
import { colorFor } from "@/lib/collab/colors";

// ---------------------------------------------------------------------------
// Avatar — unified profile image with graceful initials fallback.
//
// Why a shared component: three call-sites (header user-menu, profile page,
// team table) were each re-implementing the same `picture ? <img> : <div>`
// pattern with slightly different sizes and initial-extraction logic. That
// drift is the problem the component solves; any visual update (e.g. dark-
// mode ring colour, focus outline) now lives in exactly one place.
//
// Why `next/image`: the `no-img-element` lint gate flags raw `<img>` because
// LCP and bandwidth regress noticeably once avatars render in grids (team
// tables, member lists). `next/image` defers off-screen pixels and streams
// an appropriately-sized variant.
// ---------------------------------------------------------------------------

export type AvatarSize = "xs" | "sm" | "md" | "lg";

export interface AvatarProps {
  /** Image URL. When null/undefined, the initials fallback renders. */
  src?: string | null;
  /** Used for `alt`, for the title tooltip, and as the initials source. */
  name: string;
  size?: AvatarSize;
  className?: string;
}

const SIZE_PX: Record<AvatarSize, number> = {
  xs: 24,
  sm: 28,
  md: 40,
  lg: 64,
};

const SIZE_CLASSES: Record<AvatarSize, string> = {
  xs: "h-6 w-6 text-2xs",
  sm: "h-7 w-7 text-2xs",
  md: "h-10 w-10 text-xs",
  lg: "h-16 w-16 text-xl",
};

function getInitials(name: string): string {
  return name
    .split(/\s+/)
    .map((chunk) => chunk[0] ?? "")
    .join("")
    .toUpperCase()
    .slice(0, 2);
}

export function Avatar({ src, name, size = "md", className }: AvatarProps) {
  const px = SIZE_PX[size];
  const sizeClass = SIZE_CLASSES[size];

  if (src) {
    return (
      <Image
        src={src}
        alt={name}
        width={px}
        height={px}
        className={cn(sizeClass.split(" ").slice(0, 2).join(" "), "shrink-0 rounded-full", className)}
        // `referrerPolicy="no-referrer"` — Google / Gravatar both 403 when the
        // referrer is a non-whitelisted origin; sending no referrer keeps
        // the CDN serving public avatars.
        referrerPolicy="no-referrer"
        unoptimized
      />
    );
  }

  return (
    <div
      className={cn(
        "flex shrink-0 items-center justify-center rounded-full font-semibold text-white",
        sizeClass,
        className,
      )}
      style={{ backgroundColor: colorFor(name) }}
      aria-label={name}
    >
      {getInitials(name)}
    </div>
  );
}
