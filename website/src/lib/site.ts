export const SITE = {
	name: 'Ammini',
	version: '0.1.3',
	// Canonical origin when published (GitHub Pages project site).
	url: 'https://anjaneyasivan.github.io/ammini',
	repo: 'https://github.com/anjaneyasivan/ammini',
	get releases() {
		return `${this.repo}/releases/latest`;
	},
	get issues() {
		return `${this.repo}/issues`;
	}
} as const;
