export interface SnoozeOption {
  id: string;
  label: string;
  compute: () => number;
}

export function computeLaterToday(): number {
  const now = new Date();
  if (now.getHours() >= 18) {
    const tomorrow = new Date(now);
    tomorrow.setDate(tomorrow.getDate() + 1);
    tomorrow.setHours(9, 0, 0, 0);
    return Math.floor(tomorrow.getTime() / 1000);
  }
  return Math.floor((now.getTime() + 3 * 60 * 60 * 1000) / 1000);
}

export function computeTomorrowMorning(): number {
  const now = new Date();
  const tomorrow = new Date(now);
  tomorrow.setDate(tomorrow.getDate() + 1);
  tomorrow.setHours(9, 0, 0, 0);
  return Math.floor(tomorrow.getTime() / 1000);
}

export function computeNextWeek(): number {
  const now = new Date();
  const daysUntilMonday = (8 - now.getDay()) % 7 || 7;
  const monday = new Date(now);
  monday.setDate(monday.getDate() + daysUntilMonday);
  monday.setHours(9, 0, 0, 0);
  return Math.floor(monday.getTime() / 1000);
}

export function formatSnoozePreview(timestamp: number): string {
  const date = new Date(timestamp * 1000);
  return date.toLocaleString(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

export const snoozeOptions: SnoozeOption[] = [
  { id: "later_today", label: "Later Today", compute: computeLaterToday },
  { id: "tomorrow", label: "Tomorrow Morning", compute: computeTomorrowMorning },
  { id: "next_week", label: "Next Week", compute: computeNextWeek },
];
