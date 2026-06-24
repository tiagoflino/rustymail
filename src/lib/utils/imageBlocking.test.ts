import { describe, it, expect } from 'vitest';
import {
  hasBlockedImages,
  restoreBlockedImages,
  shouldBlockRemoteImages,
  allowsImageRestore,
} from './imageBlocking';

function makeDoc(html: string): Document {
  const doc = document.implementation.createHTMLDocument('test');
  doc.body.innerHTML = html;
  return doc;
}

describe('shouldBlockRemoteImages', () => {
  it('blocks in ask mode', () => {
    expect(shouldBlockRemoteImages('ask')).toBe(true);
  });
  it('blocks in never mode', () => {
    expect(shouldBlockRemoteImages('never')).toBe(true);
  });
  it('does not block in always mode', () => {
    expect(shouldBlockRemoteImages('always')).toBe(false);
  });
  it('does not block for null/undefined', () => {
    expect(shouldBlockRemoteImages(null)).toBe(false);
    expect(shouldBlockRemoteImages(undefined)).toBe(false);
  });
});

describe('allowsImageRestore', () => {
  it('allows restore only in ask mode', () => {
    expect(allowsImageRestore('ask')).toBe(true);
    expect(allowsImageRestore('never')).toBe(false);
    expect(allowsImageRestore('always')).toBe(false);
    expect(allowsImageRestore(null)).toBe(false);
  });
});

describe('hasBlockedImages', () => {
  it('detects a blocked src', () => {
    const doc = makeDoc('<img data-blocked-src="https://cdn.com/a.png">');
    expect(hasBlockedImages(doc)).toBe(true);
  });
  it('detects a blocked style background', () => {
    const doc = makeDoc('<div data-blocked-style="background:url(https://x/y)">x</div>');
    expect(hasBlockedImages(doc)).toBe(true);
  });
  it('returns false for clean content', () => {
    const doc = makeDoc('<img src="cid:inline"><p>hi</p>');
    expect(hasBlockedImages(doc)).toBe(false);
  });
});

describe('restoreBlockedImages', () => {
  it('restores src and removes the data-blocked attribute', () => {
    const doc = makeDoc('<img data-blocked-src="https://cdn.com/a.png" alt="x">');
    const n = restoreBlockedImages(doc);
    expect(n).toBe(1);
    const img = doc.querySelector('img')!;
    expect(img.getAttribute('src')).toBe('https://cdn.com/a.png');
    expect(img.hasAttribute('data-blocked-src')).toBe(false);
    expect(img.getAttribute('alt')).toBe('x');
  });

  it('restores srcset, poster and style together', () => {
    const doc = makeDoc(
      '<img data-blocked-srcset="//cdn/a.webp 1x">' +
      '<video data-blocked-poster="https://cdn/p.jpg"></video>' +
      '<div data-blocked-style="background:url(https://x/y);color:red">x</div>'
    );
    const n = restoreBlockedImages(doc);
    expect(n).toBe(3);
    expect(doc.querySelector('img')!.getAttribute('srcset')).toBe('//cdn/a.webp 1x');
    expect(doc.querySelector('video')!.getAttribute('poster')).toBe('https://cdn/p.jpg');
    expect(doc.querySelector('div')!.getAttribute('style')).toBe('background:url(https://x/y);color:red');
  });

  it('leaves an already-clean document untouched', () => {
    const doc = makeDoc('<img src="cid:inline"><p>hi</p>');
    const n = restoreBlockedImages(doc);
    expect(n).toBe(0);
    expect(doc.querySelector('img')!.getAttribute('src')).toBe('cid:inline');
  });

  it('makes hasBlockedImages false after restore (ask -> loaded)', () => {
    const doc = makeDoc('<img data-blocked-src="https://cdn.com/a.png">');
    expect(hasBlockedImages(doc)).toBe(true);
    restoreBlockedImages(doc);
    expect(hasBlockedImages(doc)).toBe(false);
  });
});
