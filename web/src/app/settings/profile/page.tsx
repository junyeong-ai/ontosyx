"use client";

import { useAuth } from "@/lib/use-auth";
import { Spinner } from "@/components/ui/spinner";

const ROLE_LABELS: Record<string, { label: string; color: string }> = {
  admin: {
    label: "Admin",
    color:
      "bg-indigo-100 text-indigo-700 dark:bg-indigo-900/30 dark:text-indigo-400",
  },
  designer: {
    label: "Designer",
    color:
      "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
  },
  viewer: {
    label: "Viewer",
    color:
      "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400",
  },
};

export default function ProfilePage() {
  const { user, loading, authEnabled } = useAuth();

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <Spinner size="lg" className="text-emerald-500" />
      </div>
    );
  }

  // Dev mode — no auth configured
  if (!authEnabled) {
    return (
      <div>
        <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
          Profile
        </h1>
        <div className="mt-6 rounded-lg border border-zinc-200 bg-white p-6 dark:border-zinc-800 dark:bg-zinc-900">
          <div className="flex items-center gap-3">
            <div className="flex h-12 w-12 items-center justify-center rounded-full bg-zinc-200 text-lg font-semibold text-zinc-500 dark:bg-zinc-700 dark:text-zinc-400">
              D
            </div>
            <div>
              <p className="text-sm font-medium text-zinc-900 dark:text-zinc-100">
                Developer
              </p>
              <p className="text-xs text-zinc-500 dark:text-zinc-400">
                dev@localhost
              </p>
            </div>
          </div>
          <div className="mt-4 rounded-md border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-300">
            Development mode — no authentication configured. Configure
            Google OAuth to enable user accounts.
          </div>
        </div>
      </div>
    );
  }

  if (!user) {
    return (
      <div>
        <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
          Profile
        </h1>
        <div className="mt-6 rounded-lg border border-zinc-200 bg-white p-6 text-center dark:border-zinc-800 dark:bg-zinc-900">
          <p className="text-sm text-zinc-500 dark:text-zinc-400">
            Not signed in.
          </p>
          <a
            href="/login"
            className="mt-3 inline-block rounded-md bg-emerald-600 px-4 py-2 text-sm font-medium text-white hover:bg-emerald-700"
          >
            Sign In
          </a>
        </div>
      </div>
    );
  }

  const initials = user.name
    .split(" ")
    .map((n) => n[0])
    .join("")
    .toUpperCase()
    .slice(0, 2);

  const role = user.role;
  const roleMeta = role
    ? ROLE_LABELS[role] ?? { label: role, color: "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400" }
    : undefined;

  return (
    <div>
      <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
        Profile
      </h1>

      <div className="mt-6 space-y-6">
        {/* Avatar & Identity */}
        <section className="rounded-lg border border-zinc-200 bg-white p-6 dark:border-zinc-800 dark:bg-zinc-900">
          <div className="flex items-center gap-4">
            {user.picture ? (
              <img
                src={user.picture}
                alt={user.name}
                className="h-16 w-16 rounded-full"
                referrerPolicy="no-referrer"
              />
            ) : (
              <div className="flex h-16 w-16 items-center justify-center rounded-full bg-indigo-600 text-xl font-semibold text-white">
                {initials}
              </div>
            )}
            <div>
              <p className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
                {user.name}
              </p>
              <p className="text-sm text-zinc-500 dark:text-zinc-400">
                {user.email}
              </p>
              {roleMeta && (
                <span
                  className={`mt-1 inline-block rounded-full px-2.5 py-0.5 text-xs font-medium ${roleMeta.color}`}
                >
                  {roleMeta.label}
                </span>
              )}
            </div>
          </div>
        </section>

        {/* Account Details */}
        <section className="rounded-lg border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900">
          <div className="border-b border-zinc-100 px-6 py-4 dark:border-zinc-800">
            <h2 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
              Account Details
            </h2>
          </div>
          <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
            <DetailRow label="Name" value={user.name} />
            <DetailRow label="Email" value={user.email} />
            <DetailRow label="Sign-in Provider" value="Google" />
            {role && (
              <DetailRow label="Role" value={roleMeta?.label ?? role} />
            )}
          </div>
        </section>

        {/* Sign Out */}
        <section className="rounded-lg border border-zinc-200 bg-white p-6 dark:border-zinc-800 dark:bg-zinc-900">
          <h2 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
            Session
          </h2>
          <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">
            Sign out of your current session on this device.
          </p>
          <form action="/auth/logout" method="POST" className="mt-4">
            <button
              type="submit"
              className="rounded-md border border-red-200 bg-white px-4 py-2 text-sm font-medium text-red-600 transition-colors hover:bg-red-50 dark:border-red-800 dark:bg-zinc-900 dark:text-red-400 dark:hover:bg-red-950/30"
            >
              Sign Out
            </button>
          </form>
        </section>
      </div>
    </div>
  );
}

function DetailRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between px-6 py-3">
      <span className="text-sm text-zinc-500 dark:text-zinc-400">
        {label}
      </span>
      <span className="text-sm font-medium text-zinc-900 dark:text-zinc-100">
        {value}
      </span>
    </div>
  );
}
