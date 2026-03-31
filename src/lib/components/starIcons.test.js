import { describe, it, expect } from 'vitest';
import {
	getStarIcon,
	getNextStar,
	getStarColor,
	SUPERSTAR_ORDER,
	SUPERSTAR_ICONS,
	iconStarOutline
} from './starIcons';

describe('getStarIcon', () => {
	it('returns outline for null', () => {
		expect(getStarIcon(null)).toBe(iconStarOutline);
	});

	it('returns correct icon for each star type', () => {
		for (const star of SUPERSTAR_ORDER) {
			const icon = getStarIcon(star);
			expect(icon).not.toBe(iconStarOutline);
			expect(icon).toContain('<svg');
		}
	});

	it('returns outline for unknown type', () => {
		expect(getStarIcon('UNKNOWN')).toBe(iconStarOutline);
	});
});

describe('getNextStar', () => {
	const available = ['YELLOW_STAR', 'BLUE_STAR', 'GREEN_CIRCLE'];

	it('returns first star when currently unstarred', () => {
		expect(getNextStar(null, available)).toBe('YELLOW_STAR');
	});

	it('advances to next star', () => {
		expect(getNextStar('YELLOW_STAR', available)).toBe('BLUE_STAR');
		expect(getNextStar('BLUE_STAR', available)).toBe('GREEN_CIRCLE');
	});

	it('returns null after last star (unstar)', () => {
		expect(getNextStar('GREEN_CIRCLE', available)).toBeNull();
	});

	it('returns null for unknown current star', () => {
		expect(getNextStar('UNKNOWN', available)).toBeNull();
	});

	it('returns null for empty available list', () => {
		expect(getNextStar(null, [])).toBeNull();
	});

	it('handles single star available', () => {
		expect(getNextStar(null, ['RED_STAR'])).toBe('RED_STAR');
		expect(getNextStar('RED_STAR', ['RED_STAR'])).toBeNull();
	});
});

describe('getStarColor', () => {
	it('returns correct colors', () => {
		expect(getStarColor('YELLOW_STAR')).toBe('#F4B400');
		expect(getStarColor('BLUE_STAR')).toBe('#4285F4');
		expect(getStarColor('RED_CIRCLE')).toBe('#D93025');
	});

	it('returns default for null', () => {
		expect(getStarColor(null)).toBeTruthy();
	});
});
