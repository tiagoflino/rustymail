/**
 * Remote-image blocking helpers for the email renderer.
 *
 * The Rust backend (`block_remote_images`) rewrites every remote-content load
 * vector into a recoverable placeholder by moving the original value into a
 * `data-blocked-*` attribute (`data-blocked-src`, `data-blocked-srcset`,
 * `data-blocked-poster`, `data-blocked-style`, `data-blocked-href`, ...).
 *
 * "Ask First" mode shows the blocked email and offers a "Load Images" control
 * that restores those attributes. "Never" mode uses the same backend output but
 * never exposes the restore control, so the two modes are genuinely different.
 *
 * These functions are pure DOM transforms so they can be unit-tested with jsdom.
 */

/** Map of blocked `data-*` attribute -> the live attribute it should restore to. */
const RESTORE_MAP: Record<string, string> = {
  "data-blocked-src": "src",
  "data-blocked-srcset": "srcset",
  "data-blocked-poster": "poster",
  "data-blocked-style": "style",
  "data-blocked-href": "href",
  "data-blocked-xlink:href": "xlink:href",
};

/** Whether a rendered document still contains any blocked remote content. */
export function hasBlockedImages(root: ParentNode): boolean {
  return root.querySelector("[data-blocked-src],[data-blocked-srcset],[data-blocked-poster],[data-blocked-style],[data-blocked-href]") !== null;
}

/**
 * Restore every blocked remote-content attribute in `root`, re-enabling the
 * original images/backgrounds. Returns the number of attributes restored.
 */
export function restoreBlockedImages(root: ParentNode): number {
  let restored = 0;
  for (const [blockedAttr, liveAttr] of Object.entries(RESTORE_MAP)) {
    const els = root.querySelectorAll(`[${cssEscapeAttr(blockedAttr)}]`);
    for (const el of Array.from(els)) {
      const value = el.getAttribute(blockedAttr);
      if (value !== null) {
        el.setAttribute(liveAttr, value);
        el.removeAttribute(blockedAttr);
        restored++;
      }
    }
  }
  return restored;
}

/**
 * Escape a `data-*` attribute name for use inside an attribute selector.
 * `xlink:href` contains a colon which must be escaped in a CSS selector.
 */
function cssEscapeAttr(name: string): string {
  return name.replace(/:/g, "\\:");
}

/**
 * Whether the configured image-load mode should rewrite remote content before
 * rendering. `always` renders untouched; `ask` and `never` both block.
 */
export function shouldBlockRemoteImages(mode: string | null | undefined): boolean {
  return mode === "ask" || mode === "never";
}

/**
 * Whether the renderer should expose a "Load Images" restore control. Only
 * "ask" mode is recoverable; "never" stays blocked.
 */
export function allowsImageRestore(mode: string | null | undefined): boolean {
  return mode === "ask";
}
