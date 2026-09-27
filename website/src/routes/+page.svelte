<script lang="ts">
	import Nav from '$lib/components/Nav.svelte';
	import Hero from '$lib/components/Hero.svelte';
	import Features from '$lib/components/Features.svelte';
	import TelegramSection from '$lib/components/TelegramSection.svelte';
	import Shortcuts from '$lib/components/Shortcuts.svelte';
	import Footer from '$lib/components/Footer.svelte';
	import { SITE } from '$lib/site';
</script>

<svelte:head>
	<title>Ammini — a cozy video player that streams from Telegram</title>
	<meta
		name="description"
		content="Ammini is a free, open-source video player for macOS and Windows. Play local files, URLs, and stream videos straight from your Telegram chats — with a smart disk cache and instant seeking."
	/>
	<meta name="theme-color" content="#0b0906" />
	<meta property="og:title" content="Ammini — a cozy video player that streams from Telegram" />
	<meta
		property="og:description"
		content="Play local files, URLs, and stream videos straight from your Telegram chats. Free & open source for macOS and Windows."
	/>
	<meta property="og:type" content="website" />
	<meta property="og:url" content={SITE.url} />
	<meta property="og:image" content="{SITE.url}/images/ammini-playback-screenshot.webp" />
	<meta name="twitter:card" content="summary_large_image" />
	<meta name="twitter:image" content="{SITE.url}/images/ammini-playback-screenshot.webp" />
</svelte:head>

<Nav />
<main>
	<Hero />
	<Features />
	<TelegramSection />
	<Shortcuts />

	<!-- closing CTA -->
	<section class="px-6 pb-24">
		<div
			class="relative mx-auto max-w-4xl overflow-hidden rounded-3xl border border-gold-400/20 bg-gradient-to-br from-ink-800 via-ink-900 to-ink-950 px-8 py-16 text-center sm:px-16"
		>
			<div
				class="pointer-events-none absolute -top-24 left-1/2 h-64 w-[36rem] -translate-x-1/2 rounded-full bg-gold-400/15 blur-3xl"
				aria-hidden="true"
			></div>
			<h2 class="relative font-display text-3xl font-bold tracking-tight sm:text-4xl">
				Grab the latest build
			</h2>
			<p class="relative mx-auto mt-4 max-w-xl text-cream-500">
				One download, no installer hoops. macOS (Apple Silicon) and Windows builds live on the
				GitHub releases page.
			</p>
			<div class="relative mt-8 flex flex-wrap justify-center gap-4">
				<a
					href={SITE.releases}
					target="_blank"
					rel="noreferrer"
					class="flex items-center gap-2 rounded-full bg-gold-400 px-7 py-3 font-semibold text-ink-950 shadow-[0_8px_30px_-8px] shadow-gold-400/70 transition hover:-translate-y-0.5 hover:bg-gold-300"
				>
					<iconify-icon icon="mdi:download-circle-outline" class="text-xl"></iconify-icon>
					Download from GitHub
				</a>
				<a
					href={SITE.repo}
					target="_blank"
					rel="noreferrer"
					class="flex items-center gap-2 rounded-full border border-white/15 px-7 py-3 font-semibold text-cream-100 transition hover:-translate-y-0.5 hover:border-gold-400/40"
				>
					<iconify-icon icon="simple-icons:github" class="text-lg"></iconify-icon>
					Star on GitHub
				</a>
			</div>
		</div>
	</section>
</main>
<Footer />
