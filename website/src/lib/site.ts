// The deploy workflow resolves the latest GitHub Release tag at build time and
// passes it as VITE_LATEST_RELEASE_TAG (see .env.example). Vite exposes VITE_*
// env vars to import.meta.env. Unset in local dev — download links then fall
// back to the releases/latest page.
function normalizeTag(raw: unknown): string | null {
	const tag = typeof raw === 'string' ? raw.trim() : '';
	if (!tag) return null;
	return tag.startsWith('v') ? tag : `v${tag}`;
}

const tag = normalizeTag(import.meta.env.VITE_LATEST_RELEASE_TAG);

const REPO = 'https://github.com/anjaneyasivan/ammini';

export const SITE = {
	name: 'Ammini',
	// Canonical origin when published (GitHub Pages project site).
	url: 'https://anjaneyasivan.github.io/ammini',
	repo: REPO,
	get releases() {
		return `${REPO}/releases/latest`;
	},
	get issues() {
		return `${REPO}/issues`;
	},
	// Latest release info resolved at build time, or null when no release tag
	// was available (local dev / no releases yet). Asset names must match what
	// build-macos.yml and build-windows.yml attach to the release:
	// Ammini-<version>.dmg and Ammini-<version>-x64.zip on tag v<version>.
	release: tag
		? {
				tag,
				version: tag.slice(1),
				mac: `${REPO}/releases/download/${tag}/Ammini-${tag.slice(1)}.dmg`,
				windows: `${REPO}/releases/download/${tag}/Ammini-${tag.slice(1)}-x64.zip`
			}
		: null
} as const;
