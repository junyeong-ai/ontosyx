"use client";

import { FormInput } from "@/components/ui/form-input";

// DateTimeInput — wraps `<input type="datetime-local">` with the
// ISO-8601 wire shape. The native control round-trips
// `YYYY-MM-DDTHH:mm`; the wire format is full ISO with seconds +
// timezone, so this component normalises in both directions.

interface DateTimeInputProps {
  value: string | null | undefined;
  onChange: (next: string | null) => void;
  disabled?: boolean;
  ariaInvalid?: boolean;
  ariaLabel?: string;
}

export function DateTimeInput({
  value,
  onChange,
  disabled,
  ariaInvalid,
  ariaLabel,
}: DateTimeInputProps) {
  const localValue = toLocal(value);

  return (
    <FormInput
      type="datetime-local"
      value={localValue}
      onChange={(e) => {
        const v = e.target.value;
        onChange(v ? toIso(v) : null);
      }}
      density="compact"
      disabled={disabled}
      aria-invalid={ariaInvalid}
      aria-label={ariaLabel}
    />
  );
}

function toLocal(iso: string | null | undefined): string {
  if (!iso) return "";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const pad = (n: number) => String(n).padStart(2, "0");
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}` +
    `T${pad(date.getHours())}:${pad(date.getMinutes())}`
  );
}

function toIso(local: string): string {
  const date = new Date(local);
  if (Number.isNaN(date.getTime())) return new Date().toISOString();
  return date.toISOString();
}
