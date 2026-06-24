export type TrackerType = "tracking_pixel" | "remote_image" | "read_receipt";

export interface DetectedTracker {
	tracker_type: TrackerType;
	details: string;
	url_snippet: string;
	blocked: boolean;
}

export interface TrackingScanResult {
	trackers_found: number;
	trackers_blocked: number;
	cleaned_html: string;
	tracker_details: DetectedTracker[];
}

export interface ScanOutcome {
	html: string;
	scanFailed: boolean;
	result: TrackingScanResult | null;
}

type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

/**
 * Scan an email body for trackers and return HTML safe to render.
 * Gating: only invokes the scan when the block_tracking_pixels setting is "true".
 * Fail-closed: if scanning is enabled but the scan command errors, the original
 * HTML is returned with scanFailed=true so the caller can surface a signal
 * instead of silently rendering live trackers.
 */
export async function scanForDisplay(
	invoke: InvokeFn,
	html: string,
	sender: string | null,
	messageId: string | null
): Promise<ScanOutcome> {
	let blockPixels: string;
	try {
		blockPixels = await invoke<string>("get_setting", { key: "block_tracking_pixels" });
	} catch {
		return { html, scanFailed: false, result: null };
	}

	if (blockPixels !== "true") {
		return { html, scanFailed: false, result: null };
	}

	try {
		const result = await invoke<TrackingScanResult>("scan_tracking_content", {
			html,
			sender,
			messageId
		});
		return { html: result.cleaned_html, scanFailed: false, result };
	} catch {
		return { html, scanFailed: true, result: null };
	}
}
