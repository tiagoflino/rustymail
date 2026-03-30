import { invoke } from "@tauri-apps/api/core";

export interface ShortcutMap {
  [action: string]: string;
}

export class ShortcutManager {
  private shortcuts: ShortcutMap = {
    palette: "Meta+k",
    compose: "c",
    sync: "Meta+r",
    settings: "Meta+,",
    search: "/",
    sidebar: "[",
    escape: "Escape"
  };
  private listeners: { [action: string]: Array<() => void> } = {};

  async loadSettings() {
    // No longer required but kept for compatibility
  }

  on(action: string, callback: () => void) {
    if (!this.listeners[action]) this.listeners[action] = [];
    this.listeners[action].push(callback);
    return () => {
      this.listeners[action] = this.listeners[action].filter(cb => cb !== callback);
    };
  }

  trigger(action: string) {
    if (this.listeners[action]) {
      this.listeners[action].forEach(cb => cb());
    }
  }

  handleKeydown = (e: KeyboardEvent) => {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || (e.target as HTMLElement).isContentEditable) {
      return;
    }

    const key = this.serializeEvent(e);
    
    for (const [action, shortcutStr] of Object.entries(this.shortcuts)) {
      if (this.matchShortcut(key, shortcutStr)) {
        e.preventDefault();
        this.trigger(action);
        return;
      }
    }
  };

  private serializeEvent(e: KeyboardEvent): string {
    let parts = [];
    if (e.metaKey || e.ctrlKey) parts.push("Meta");
    if (e.shiftKey) parts.push("Shift");
    if (e.altKey) parts.push("Alt");
    
    if (!["Meta", "Control", "Shift", "Alt"].includes(e.key)) {
      parts.push(e.key.toLowerCase());
    }
    
    return parts.join("+");
  }

  private matchShortcut(pressed: string, configured: string): boolean {
    // Basic normalized equal
    return pressed.toLowerCase() === configured.toLowerCase() || 
           (pressed.replace("Meta", "Ctrl").toLowerCase() === configured.replace("Meta", "Ctrl").toLowerCase());
  }
}

export const shortcutManager = new ShortcutManager();
