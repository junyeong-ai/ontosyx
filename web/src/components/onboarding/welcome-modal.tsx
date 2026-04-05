"use client";

import { useState, useEffect } from "react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/cn";

const STORAGE_KEY = "ontosyx.onboarded";

const STEPS = [
  {
    title: "Connect Your Data",
    description:
      "Link any database — PostgreSQL, MySQL, MongoDB, or upload CSV/JSON files. Ontosyx analyzes your schema automatically.",
    icon: "\u{1F517}",
  },
  {
    title: "Design Your Ontology",
    description:
      "AI designs a knowledge graph from your data structure. Refine nodes, edges, and properties visually on the canvas.",
    icon: "\u{1F9E0}",
  },
  {
    title: "Discover Insights",
    description:
      "Ask natural language questions to explore multi-hop relationships impossible in traditional SQL. Build dashboards and reports.",
    icon: "\u{1F4A1}",
  },
];

export function WelcomeModal() {
  const [step, setStep] = useState(0);
  const [show, setShow] = useState(false);

  useEffect(() => {
    if (typeof window !== "undefined" && !localStorage.getItem(STORAGE_KEY)) {
      setShow(true);
    }
  }, []);

  const dismiss = () => {
    localStorage.setItem(STORAGE_KEY, "true");
    setShow(false);
  };

  if (!show) return null;

  const current = STEPS[step];
  const isLast = step === STEPS.length - 1;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-full max-w-md rounded-xl bg-white p-8 shadow-2xl dark:bg-zinc-900">
        {/* Step indicator */}
        <div className="mb-6 flex justify-center gap-2">
          {STEPS.map((_, i) => (
            <div
              key={i}
              className={cn(
                "h-1.5 w-8 rounded-full",
                i === step
                  ? "bg-emerald-500"
                  : "bg-zinc-200 dark:bg-zinc-700",
              )}
            />
          ))}
        </div>

        {/* Content */}
        <div className="text-center">
          <span className="text-4xl">{current.icon}</span>
          <h2 className="mt-4 text-lg font-semibold text-zinc-900 dark:text-zinc-100">
            {current.title}
          </h2>
          <p className="mt-2 text-sm text-zinc-500 dark:text-zinc-400">
            {current.description}
          </p>
        </div>

        {/* Actions */}
        <div className="mt-8 flex items-center justify-between">
          <button
            onClick={dismiss}
            className="text-xs text-zinc-400 hover:text-zinc-600"
          >
            Skip
          </button>
          <Button
            variant="primary"
            size="sm"
            onClick={isLast ? dismiss : () => setStep((s) => s + 1)}
          >
            {isLast ? "Get Started" : "Next"}
          </Button>
        </div>
      </div>
    </div>
  );
}
