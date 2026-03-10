import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { addToast } from "$lib/stores/toast";

let pendingUpdate: Update | null = null;
let isInstalling = false;

export async function checkForUpdates(silent: boolean = true): Promise<void> {
    try {
        const update = await check();
        if (update) {
            pendingUpdate = update;
            addToast(
                `Rustymail ${update.version} is available`,
                "info",
                0,
                {
                    label: "Update",
                    onClick: () => installAndRestart(),
                }
            );
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

async function installAndRestart(): Promise<void> {
    if (!pendingUpdate || isInstalling) return;
    isInstalling = true;

    addToast("Downloading update...", "info", 0);

    try {
        await pendingUpdate.downloadAndInstall();
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
