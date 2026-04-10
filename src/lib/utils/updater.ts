import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";
import { addToast } from "$lib/stores/toast";

import { writable, get } from "svelte/store";

export interface UpdateInfo {
    currentVersion: string;
    newVersion: string;
    releaseDate: string | null;
    releaseNotes: string | null;
    onInstall: () => void;
}

export const pendingUpdate = writable<UpdateInfo | null>(null);
let isInstalling = false;

export async function checkForUpdates(silent: boolean = true): Promise<void> {
    try {
        const update = await check();
        const currentVersion = await getVersion();
        
        if (update) {
            pendingUpdate.set({
                currentVersion,
                newVersion: update.version,
                releaseDate: update.date,
                releaseNotes: update.body,
                onInstall: () => installAndRestart(update),
            });
        } else if (!silent) {
            addToast("You're on the latest version", "success", 3000);
        }
    } catch (error) {
        console.error("Update check failed:", error);
        if (!silent) {
            addToast("Could not check for updates", "error", 4000);
        }
    }
}

export async function installAndRestart(update?: Update): Promise<void> {
    const pending = get(pendingUpdate);
    const updateToInstall = update || pending;
    if (!updateToInstall || isInstalling) return;
    isInstalling = true;

    addToast("Downloading update...", "info", 0);

    try {
        await updateToInstall.downloadAndInstall();
        await relaunch();
    } catch (error) {
        console.error("Update install failed:", error);
        isInstalling = false;
        addToast("Update failed. Please try again later.", "error", 5000);
    }
}

export function setupPeriodicUpdateCheck(intervalMs: number = 4 * 60 * 60 * 1000): () => void {
    const id = setInterval(() => checkForUpdates(true), intervalMs);
    return () => clearInterval(id);
}
