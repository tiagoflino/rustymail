import { describe, it, expect, vi } from "vitest";
import { scanForDisplay, type TrackingScanResult } from "./trackingScan";

function makeInvoke(handlers: Record<string, (args: any) => unknown>) {
	return vi.fn(async (cmd: string, args?: any) => {
		const h = handlers[cmd];
		if (!h) throw new Error(`Unknown command: ${cmd}`);
		return h(args);
	}) as unknown as <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
}

const scanResult: TrackingScanResult = {
	trackers_found: 1,
	trackers_blocked: 1,
	cleaned_html: "<p>clean</p>",
	tracker_details: [
		{ tracker_type: "tracking_pixel", details: "1x1 tracking pixel", url_snippet: "x", blocked: true }
	]
};

describe("scanForDisplay", () => {
	it("returns original html and does NOT scan when blocking is disabled", async () => {
		const invoke = makeInvoke({
			get_setting: () => "false",
			scan_tracking_content: () => scanResult
		});
		const out = await scanForDisplay(invoke, "<p>raw</p>", "a@b.com", "m1");
		expect(out.html).toBe("<p>raw</p>");
		expect(out.scanFailed).toBe(false);
		expect(out.result).toBeNull();
		expect(invoke).toHaveBeenCalledWith("get_setting", { key: "block_tracking_pixels" });
		expect(invoke).not.toHaveBeenCalledWith("scan_tracking_content", expect.anything());
	});

	it("scans and returns cleaned html when blocking is enabled", async () => {
		const invoke = makeInvoke({
			get_setting: () => "true",
			scan_tracking_content: () => scanResult
		});
		const out = await scanForDisplay(invoke, "<img src=x width=1 height=1>", "a@b.com", "m1");
		expect(out.html).toBe("<p>clean</p>");
		expect(out.scanFailed).toBe(false);
		expect(out.result).toEqual(scanResult);
	});

	it("passes sender and messageId through to the scan command", async () => {
		const invoke = makeInvoke({
			get_setting: () => "true",
			scan_tracking_content: () => scanResult
		});
		await scanForDisplay(invoke, "<p>x</p>", "spy@example.com", "msg-42");
		expect(invoke).toHaveBeenCalledWith("scan_tracking_content", {
			html: "<p>x</p>",
			sender: "spy@example.com",
			messageId: "msg-42"
		});
	});

	it("fails closed: scan error sets scanFailed and does not silently render trackers as clean", async () => {
		const invoke = makeInvoke({
			get_setting: () => "true",
			scan_tracking_content: () => {
				throw new Error("command rejected");
			}
		});
		const raw = "<img src='https://track.com/p.gif' width=1 height=1>";
		const out = await scanForDisplay(invoke, raw, "a@b.com", "m1");
		expect(out.scanFailed).toBe(true);
		expect(out.result).toBeNull();
		expect(out.html).toBe(raw);
	});

	it("does not throw or signal failure when the setting lookup itself fails", async () => {
		const invoke = makeInvoke({
			get_setting: () => {
				throw new Error("no setting");
			}
		});
		const out = await scanForDisplay(invoke, "<p>x</p>", null, null);
		expect(out.scanFailed).toBe(false);
		expect(out.html).toBe("<p>x</p>");
	});
});
