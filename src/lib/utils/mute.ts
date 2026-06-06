export interface MuteOption {
  id: string;
  label: string;
  description: string;
  compute: () => number | null; // null = forever
}

export function computeMute24h(): number {
  return Math.floor((Date.now() + 24 * 60 * 60 * 1000) / 1000);
}

export function computeMute3d(): number {
  return Math.floor((Date.now() + 3 * 24 * 60 * 60 * 1000) / 1000);
}

export function computeMute1w(): number {
  return Math.floor((Date.now() + 7 * 24 * 60 * 60 * 1000) / 1000);
}

export function computeMuteForever(): null {
  return null;
}

export function formatMuteExpiry(timestamp: number | null): string {
  if (timestamp === null) return "Forever";
  const date = new Date(timestamp * 1000);
  return date.toLocaleString(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

export const muteOptions: MuteOption[] = [
  { id: "24h", label: "24 Hours", description: "Mute for one day", compute: computeMute24h },
  { id: "3d", label: "3 Days", description: "Mute for three days", compute: computeMute3d },
  { id: "1w", label: "1 Week", description: "Mute for one week", compute: computeMute1w },
  { id: "forever", label: "Forever", description: "Mute permanently", compute: computeMuteForever },
];
