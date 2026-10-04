<script lang="ts">
	import { base } from '$app/paths';
	import { page } from '$app/state';
	import { SITE } from '$lib/site';

	// On the landing page the section anchors are plain hashes; from any other
	// route (e.g. /privacy) they must point back to the home page.
	const onHome = $derived(page.url.pathname === base || page.url.pathname === `${base}/`);
	const anchor = (id: string) => (onHome ? `#${id}` : `${base}/#${id}`);
</script>

<header class="fixed inset-x-0 top-0 z-50">
	<div
		class="mx-auto flex max-w-6xl items-center justify-between gap-4 border-b border-white/5 bg-ink-950/70 px-6 py-3.5 backdrop-blur-md lg:rounded-b-2xl"
	>
		<a href={anchor('top')} class="flex items-center gap-2.5">
			<img
				src="{base}/images/ammini-logo.webp"
				alt="Ammini logo"
				class="size-8 rounded-full"
				width="32"
				height="32"
			/>
			<span class="font-display text-lg font-bold tracking-tight"> Ammini </span>
			<span class="hidden font-malayalam text-sm text-gold-400/80 sm:inline">അമ്മിണി</span>
		</a>

		<nav class="hidden items-center gap-7 text-sm text-cream-300 md:flex">
			<a href={anchor('features')} class="transition hover:text-gold-300">Features</a>
			<a href={anchor('telegram')} class="transition hover:text-gold-300">Telegram</a>
			<a href={anchor('shortcuts')} class="transition hover:text-gold-300">Shortcuts</a>
			<a
				href={SITE.repo}
				target="_blank"
				rel="noreferrer"
				class="flex items-center gap-1.5 transition hover:text-gold-300"
			>
				<iconify-icon icon="simple-icons:github"></iconify-icon>
				GitHub
			</a>
		</nav>

		<a
			href={SITE.releases}
			target="_blank"
			rel="noreferrer"
			class="rounded-full bg-gold-400 px-4 py-1.5 text-sm font-semibold text-ink-950 shadow-[0_0_18px_-4px] shadow-gold-400/60 transition hover:bg-gold-300"
		>
			Download
		</a>
	</div>
</header>
