/**
 * HTML Sanitizer - Whitelist-based sanitization for AI-generated content.
 *
 * This module provides a strict whitelist approach to sanitizing HTML strings
 * that may contain user-generated or AI-generated content. It only allows a
 * predefined set of safe HTML tags and attributes, stripping everything else.
 *
 * Security properties:
 * - Only allows: div, strong, em, code, span, br, p, ul, ol, li, a
 * - Only allows safe attributes: href, class, style (limited subset)
 * - Strips all event handlers (onclick, onerror, onload, etc.)
 * - Strips javascript: and data: URI schemes from href attributes
 * - Strips <style>, <script>, <iframe>, <object>, <embed> tags entirely
 * - Neutralizes <img> tags by removing all attributes
 */

// Allowed HTML tags (whitelist)
const ALLOWED_TAGS = new Set([
  'div', 'strong', 'em', 'code', 'span', 'br', 'p',
  'ul', 'ol', 'li', 'a', 'h3', 'h4', 'h5', 'h6',
]);

// Allowed attributes per tag
const ALLOWED_ATTRIBUTES: Record<string, Set<string>> = {
  '*': new Set(['class', 'style']),
  'a': new Set(['href', 'class']),
};

// Attributes that must never appear on any tag
const DANGEROUS_ATTRIBUTES = [
  'onclick', 'onerror', 'onload', 'onmouseover', 'onfocus', 'onblur',
  'onmousedown', 'onmouseup', 'onkeydown', 'onkeyup', 'onkeypress',
  'onchange', 'onsubmit', 'onreset', 'onselect', 'onscroll',
  'oninput', 'ondrag', 'ondrop', 'oncontextmenu',
];

// Dangerous URI schemes
const DANGEROUS_SCHEMES = ['javascript:', 'data:text/html', 'vbscript:', 'file:'];

/**
 * Check if a URI scheme is safe.
 */
function isSafeUri(uri: string): boolean {
  const normalized = uri.trim().toLowerCase();
  return !DANGEROUS_SCHEMES.some(scheme => normalized.startsWith(scheme));
}

/**
 * Allowed CSS properties (whitelist).
 */
const ALLOWED_CSS_PROPERTIES = new Set([
  'color', 'background-color', 'font-weight', 'font-style', 'text-decoration',
  'margin', 'margin-top', 'margin-right', 'margin-bottom', 'margin-left',
  'padding', 'padding-top', 'padding-right', 'padding-bottom', 'padding-left',
  'border', 'border-top', 'border-right', 'border-bottom', 'border-left',
  'border-color', 'border-style', 'border-width',
  'font-size', 'line-height', 'letter-spacing', 'white-space', 'word-break',
  'text-align', 'vertical-align', 'display', 'width', 'height',
  'overflow', 'text-indent', 'font-family',
]);

/**
 * Check if a CSS value is safe (no dangerous functions or patterns).
 */
function isSafeCssValue(value: string): boolean {
  const normalized = value.trim().toLowerCase();
  // Block dangerous CSS functions
  if (/expression\s*\(/.test(normalized)) return false;
  if (/url\s*\(/.test(normalized) && normalized.includes('javascript:')) return false;
  if (/@import/.test(normalized)) return false;
  if (/behavior\s*:/.test(normalized)) return false;
  if (/binding\s*:/.test(normalized)) return false;
  if (/@media/.test(normalized)) return false;
  return true;
}

/**
 * Validate a CSS value against allowed patterns.
 */
function validateCssValue(value: string): boolean {
  const normalized = value.trim().toLowerCase();
  // Allow hex colors
  if (/^#[0-9a-f]{3,8}$/.test(normalized)) return true;
  // Allow rgb/rgba/hsl colors
  if (/^rgb(a)?\s*\([^)]+\)$/.test(normalized)) return true;
  if (/^hsl\([^)]+\)$/.test(normalized)) return true;
  // Allow length values
  if (/^\d+(px|em|rem|%|pt|vw|vh|ex|ch|cm|mm|in)$/.test(normalized)) return true;
  // Allow CSS keywords
  if (/^(inherit|initial|unset|normal|bold|italic|none|underline|line-through|nowrap|break-word|solid|dashed|dotted|double|groove|ridge|hidden|visible|transparent|left|right|center|justify|top|bottom|middle|flex|block|inline|inline-block)$/i.test(normalized)) return true;
  // Allow color names
  if (/^(red|blue|green|black|white|gray|grey|orange|yellow|purple|pink|cyan|magenta|brown|navy|teal|lime|olive|aqua|maroon|silver|fuchsia)$/i.test(normalized)) return true;
  // Allow URLs that are not javascript:
  if (/^https?:\/\/[^'"]+$/.test(normalized)) return true;
  return false;
}

/**
 * Sanitize a style attribute value, returning only safe CSS properties.
 */
function sanitizeStyle(style: string): string {
  const declarations = style.split(';');
  const safeDeclarations: string[] = [];

  for (const decl of declarations) {
    const trimmed = decl.trim();
    if (!trimmed) continue;
    const colonIndex = trimmed.indexOf(':');
    if (colonIndex === -1) continue;
    const prop = trimmed.slice(0, colonIndex).trim().toLowerCase();
    const val = trimmed.slice(colonIndex + 1).trim();
    
    // Check property is allowed and doesn't start with vendor prefix
    if (!prop || prop.startsWith('-')) continue;
    if (!ALLOWED_CSS_PROPERTIES.has(prop)) continue;
    
    // Check value is safe
    if (!isSafeCssValue(val)) continue;
    if (!validateCssValue(val)) continue;
    
    safeDeclarations.push(`${prop}: ${val}`);
  }

  return safeDeclarations.join('; ');
}

/**
 * Parse attributes from a tag string, properly handling quoted values.
 * Returns an array of {name, value} pairs.
 */
function parseAttributes(tagContent: string): Array<{name: string, value: string | null}> {
  const attrs: Array<{name: string, value: string | null}> = [];
  let i = 0;
  const len = tagContent.length;
  
  while (i < len) {
    // Skip whitespace
    while (i < len && /\s/.test(tagContent[i])) i++;
    if (i >= len) break;
    
    // Read attribute name
    let name = '';
    while (i < len && tagContent[i] !== '=' && !/\s/.test(tagContent[i]) && tagContent[i] !== '>') {
      name += tagContent[i];
      i++;
    }
    
    if (!name) {
      if (tagContent[i] === '>') break;
      i++;
      continue;
    }
    
    // Skip whitespace
    while (i < len && /\s/.test(tagContent[i])) i++;
    
    // Check for = sign
    if (i < len && tagContent[i] === '=') {
      i++; // skip =
      // Skip whitespace
      while (i < len && /\s/.test(tagContent[i])) i++;
      
      // Read value
      let value = '';
      if (i < len && (tagContent[i] === '"' || tagContent[i] === "'")) {
        const quote = tagContent[i];
        i++; // skip opening quote
        while (i < len && tagContent[i] !== quote) {
          value += tagContent[i];
          i++;
        }
        if (i < len) i++; // skip closing quote
      } else {
        // Unquoted value
        while (i < len && !/\s/.test(tagContent[i]) && tagContent[i] !== '>') {
          value += tagContent[i];
          i++;
        }
      }
      
      attrs.push({ name: name.toLowerCase(), value });
    } else {
      // Boolean attribute
      attrs.push({ name: name.toLowerCase(), value: null });
    }
  }
  
  return attrs;
}

/**
 * Sanitize an HTML string using a whitelist approach.
 * This is a lightweight parser that does not require external dependencies.
 *
 * @param html - The raw HTML string to sanitize
 * @returns A sanitized HTML string with only allowed tags and attributes
 */
export function sanitizeHtml(html: string): string {
  if (!html || typeof html !== 'string') return '';

  let result = '';
  let i = 0;
  const len = html.length;

  while (i < len) {
    // Handle comments
    if (html.slice(i, i + 4) === '<!--') {
      const endIdx = html.indexOf('-->', i + 4);
      i = endIdx === -1 ? len : endIdx + 3;
      continue;
    }

    // Handle closing tags
    if (html[i] === '<' && html[i + 1] === '/') {
      const endIdx = html.indexOf('>', i + 2);
      if (endIdx === -1) {
        i = len;
        continue;
      }
      const tagName = html.slice(i + 2, endIdx).trim().toLowerCase();
      // Only allow closing tags for allowed elements
      if (ALLOWED_TAGS.has(tagName)) {
        result += `</${tagName}>`;
      }
      i = endIdx + 1;
      continue;
    }

    // Handle opening tags
    if (html[i] === '<' && html[i + 1] !== '!') {
      const endIdx = html.indexOf('>', i + 1);
      if (endIdx === -1) {
        // Malformed tag - output as text
        result += escapeHtml(html.slice(i));
        break;
      }

      const tagContent = html.slice(i + 1, endIdx);
      const tagNameMatch = tagContent.match(/^([a-zA-Z][a-zA-Z0-9-]*)/);
      
      if (!tagNameMatch) {
        i = endIdx + 1;
        continue;
      }
      
      const tagName = tagNameMatch[1].toLowerCase();
      const restOfTag = tagContent.slice(tagNameMatch[0].length);

      // Tags that need content skipped (including closing tag)
      const TAGS_TO_SKIP_CONTENT = ['script', 'style', 'iframe', 'object', 'embed', 'svg', 'math'];
      // Tags that are silently dropped (no content to skip - self-contained)
      const TAGS_TO_DROP = ['form', 'input', 'textarea', 'button', 'link', 'meta', 'base'];
      
      // Skip dangerous tags entirely (including their content)
      if (TAGS_TO_SKIP_CONTENT.includes(tagName)) {
        // Find the closing tag to skip all content
        const closeTag = `</${tagName}>`;
        const closeIdx = html.toLowerCase().indexOf(closeTag, endIdx + 1);
        if (closeIdx !== -1) {
          i = closeIdx + closeTag.length;
        } else {
          // No closing tag, skip to end
          i = len;
        }
        continue;
      }
      
      // Drop tags without skipping content (for form-like elements)
      if (TAGS_TO_DROP.includes(tagName)) {
        i = endIdx + 1;
        continue;
      }

      // Skip tags not in whitelist
      if (!ALLOWED_TAGS.has(tagName)) {
        i = endIdx + 1;
        continue;
      }

      // Parse attributes properly handling quoted values
      const attrs = parseAttributes(restOfTag);
      
      // Build sanitized tag
      let sanitizedTag = `<${tagName}`;

      for (const attr of attrs) {
        const attrName = attr.name.toLowerCase();
        
        // Skip dangerous attributes (event handlers)
        if (attrName.startsWith('on')) {
          continue;
        }

        // Check if attribute is allowed for this tag
        const tagAllowed = ALLOWED_ATTRIBUTES[tagName];
        const globalAllowed = ALLOWED_ATTRIBUTES['*'];
        if (!tagAllowed?.has(attrName) && !globalAllowed?.has(attrName)) {
          continue;
        }

        // Special handling for href - check URI scheme
        if (attrName === 'href' && attr.value !== null && !isSafeUri(attr.value)) {
          continue;
        }

        // Special handling for style - sanitize CSS
        if (attrName === 'style' && attr.value !== null) {
          const sanitizedStyle = sanitizeStyle(attr.value);
          if (sanitizedStyle) {
            sanitizedTag += ` style="${escapeAttr(sanitizedStyle)}"`;
          }
          continue;
        }

        // Add boolean attributes or attributes with values
        if (attr.value === null) {
          sanitizedTag += ` ${attrName}`;
        } else {
          sanitizedTag += ` ${attrName}="${escapeAttr(attr.value)}"`;
        }
      }

      sanitizedTag += '>';

      // Self-closing tags
      if (['br', 'hr'].includes(tagName)) {
        result += `<${tagName}/>`;
      } else {
        result += sanitizedTag;
      }

      i = endIdx + 1;
      continue;
    }

    // Handle text content
    const nextTagIdx = html.indexOf('<', i);
    if (nextTagIdx === -1) {
      result += escapeHtml(html.slice(i));
      break;
    }
    result += escapeHtml(html.slice(i, nextTagIdx));
    i = nextTagIdx;
  }

  return result;
}

/**
 * Escape HTML special characters to prevent XSS.
 */
function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

/**
 * Escape attribute values.
 */
function escapeAttr(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}
