export const iconStarOutline = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/></svg>`;

const starPolygon = `points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"`;

function coloredStar(fill: string): string {
	return `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="${fill}" stroke="${fill}" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polygon ${starPolygon}/></svg>`;
}

function squareIcon(bg: string, symbol: string): string {
	return `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24"><rect x="2" y="2" width="20" height="20" rx="2" fill="${bg}"/><text x="12" y="17" text-anchor="middle" fill="white" font-size="16" font-weight="bold" font-family="Arial, sans-serif">${symbol}</text></svg>`;
}

const iconYellowStar = coloredStar('#F4B400');
const iconOrangeStar = coloredStar('#E67C00');
const iconRedStar = coloredStar('#D93025');
const iconPurpleStar = coloredStar('#A142F4');
const iconBlueStar = coloredStar('#4285F4');
const iconGreenStar = coloredStar('#0F9D58');

const iconGreenCircle = squareIcon('#0F9D58', '&#x2713;');
const iconRedCircle = squareIcon('#D93025', '!');
const iconOrangeCircle = squareIcon('#E67C00', '&#xBB;');
const iconYellowCircle = squareIcon('#F4B400', '!');
const iconBlueCircle = squareIcon('#4285F4', 'i');
const iconPurpleCircle = squareIcon('#A142F4', '?');

export const SUPERSTAR_ORDER: string[] = [
	'YELLOW_STAR',
	'ORANGE_STAR',
	'RED_STAR',
	'PURPLE_STAR',
	'BLUE_STAR',
	'GREEN_STAR',
	'GREEN_CIRCLE',
	'RED_CIRCLE',
	'ORANGE_CIRCLE',
	'YELLOW_CIRCLE',
	'BLUE_CIRCLE',
	'PURPLE_CIRCLE'
];

export const SUPERSTAR_ICONS: Record<string, string> = {
	YELLOW_STAR: iconYellowStar,
	ORANGE_STAR: iconOrangeStar,
	RED_STAR: iconRedStar,
	PURPLE_STAR: iconPurpleStar,
	BLUE_STAR: iconBlueStar,
	GREEN_STAR: iconGreenStar,
	GREEN_CIRCLE: iconGreenCircle,
	RED_CIRCLE: iconRedCircle,
	ORANGE_CIRCLE: iconOrangeCircle,
	YELLOW_CIRCLE: iconYellowCircle,
	BLUE_CIRCLE: iconBlueCircle,
	PURPLE_CIRCLE: iconPurpleCircle
};

const STAR_COLORS: Record<string, string> = {
	YELLOW_STAR: '#F4B400',
	ORANGE_STAR: '#E67C00',
	RED_STAR: '#D93025',
	PURPLE_STAR: '#A142F4',
	BLUE_STAR: '#4285F4',
	GREEN_STAR: '#0F9D58',
	GREEN_CIRCLE: '#0F9D58',
	RED_CIRCLE: '#D93025',
	ORANGE_CIRCLE: '#E67C00',
	YELLOW_CIRCLE: '#F4B400',
	BLUE_CIRCLE: '#4285F4',
	PURPLE_CIRCLE: '#A142F4'
};

const DEFAULT_COLOR = '#9AA0A6';

export function getStarIcon(starType: string | null): string {
	if (!starType) return iconStarOutline;
	return SUPERSTAR_ICONS[starType] || iconStarOutline;
}

export function getStarColor(starType: string | null): string {
	if (!starType) return DEFAULT_COLOR;
	return STAR_COLORS[starType] || DEFAULT_COLOR;
}

export function getNextStar(current: string | null, available: string[]): string | null {
	if (!available.length) return null;
	if (!current) return available[0];
	const idx = available.indexOf(current);
	if (idx === -1 || idx === available.length - 1) return null;
	return available[idx + 1];
}
