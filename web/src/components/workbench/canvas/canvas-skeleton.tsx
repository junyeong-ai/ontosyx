/**
 * Graph-shaped loading skeleton shown while ELK layout is being computed.
 */
export function CanvasSkeleton({ visible }: { visible: boolean }) {
  return (
    <div
      className={`pointer-events-none absolute inset-0 z-50 flex items-center justify-center bg-zinc-50 transition-opacity duration-300 dark:bg-zinc-950 ${
        visible ? "opacity-100" : "opacity-0"
      }`}
    >
      <svg
        viewBox="0 0 600 400"
        className="h-auto w-full max-w-[520px] animate-pulse"
        fill="none"
      >
        {/* Edges (lines between nodes) */}
        <line x1="150" y1="80" x2="300" y2="160" className="stroke-zinc-300 dark:stroke-zinc-700" strokeWidth="2" />
        <line x1="450" y1="80" x2="300" y2="160" className="stroke-zinc-300 dark:stroke-zinc-700" strokeWidth="2" />
        <line x1="300" y1="160" x2="180" y2="280" className="stroke-zinc-300 dark:stroke-zinc-700" strokeWidth="2" />
        <line x1="300" y1="160" x2="420" y2="280" className="stroke-zinc-300 dark:stroke-zinc-700" strokeWidth="2" />
        <line x1="180" y1="280" x2="300" y2="350" className="stroke-zinc-300 dark:stroke-zinc-700" strokeWidth="2" />
        <line x1="420" y1="280" x2="300" y2="350" className="stroke-zinc-300 dark:stroke-zinc-700" strokeWidth="2" />

        {/* Node placeholders */}
        <rect x="100" y="50" width="100" height="50" rx="8" className="fill-zinc-200 dark:fill-zinc-800" />
        <rect x="400" y="50" width="100" height="50" rx="8" className="fill-zinc-200 dark:fill-zinc-800" />
        <rect x="245" y="130" width="110" height="55" rx="8" className="fill-zinc-200 dark:fill-zinc-800" />
        <rect x="120" y="255" width="120" height="50" rx="8" className="fill-zinc-200 dark:fill-zinc-800" />
        <rect x="360" y="255" width="120" height="50" rx="8" className="fill-zinc-200 dark:fill-zinc-800" />
        <rect x="240" y="325" width="120" height="50" rx="8" className="fill-zinc-200 dark:fill-zinc-800" />
      </svg>
    </div>
  );
}
