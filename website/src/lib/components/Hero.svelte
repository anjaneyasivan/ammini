<script lang="ts">
	import { base } from '$app/paths';
	import { SITE } from '$lib/site';

	const downloads = [
		{
			os: 'macOS',
			icon: 'simple-icons:apple',
			note: SITE.release ? `Apple Silicon · v${SITE.release.version} .dmg` : 'Apple Silicon · .dmg',
			primary: true,
			href: SITE.release?.mac ?? SITE.releases
		},
		{
			os: 'Windows',
			icon: 'simple-icons:windows',
			note: SITE.release ? `Windows 10/11 · v${SITE.release.version} x64` : 'Windows 10/11 · x64',
			primary: false,
			href: SITE.release?.windows ?? SITE.releases
		}
	];
</script>

<section id="top" class="relative overflow-hidden pt-36 pb-20 sm:pt-44">
	<!-- warm glow backdrop -->
	<div class="pointer-events-none absolute inset-0 -z-10" aria-hidden="true">
		<div
			class="absolute top-[-20%] left-1/2 h-[42rem] w-[72rem] -translate-x-1/2 rounded-full bg-[radial-gradient(closest-side,rgba(238,188,95,0.14),transparent)]"
		></div>
		<div
			class="absolute top-40 left-[8%] h-72 w-72 rounded-full bg-[radial-gradient(closest-side,rgba(63,169,173,0.12),transparent)]"
		></div>
		<div
			class="absolute top-64 right-[6%] h-80 w-80 rounded-full bg-[radial-gradient(closest-side,rgba(221,164,55,0.1),transparent)]"
		></div>
	</div>

	<div class="mx-auto max-w-6xl px-6 text-center">
		<a
			href={SITE.repo}
			target="_blank"
			rel="noreferrer"
			class="inline-flex items-center gap-2 rounded-full border border-gold-400/25 bg-gold-400/10 px-4 py-1.5 text-xs font-medium tracking-wide text-gold-300 transition hover:border-gold-400/50"
		>
			<iconify-icon icon="mdi:open-source-initiative"></iconify-icon>
			Free & open source{#if SITE.release}&nbsp;· v{SITE.release.version}{/if}
		</a>

		<h1
			class="mx-auto mt-7 max-w-3xl font-display text-5xl font-extrabold tracking-tight text-balance sm:text-6xl lg:text-7xl"
		>
			Your videos, wherever they
			<span
				class="bg-gradient-to-r from-gold-300 via-gold-400 to-lagoon-400 bg-clip-text text-transparent"
				>live</span
			>
		</h1>

		<p class="mx-auto mt-6 max-w-2xl text-lg leading-relaxed text-pretty text-cream-500">
			Ammini is a cozy, keyboard-first video player for macOS and Windows that streams straight out
			of your Telegram chats — with a smart disk cache, instant seeking, and none of the fuss.
		</p>

		<div class="mt-10 flex flex-col items-center justify-center gap-4 sm:flex-row">
			{#each downloads as d (d.os)}
				<a
					href={d.href}
					target="_blank"
					rel="noreferrer"
					class="group flex w-64 items-center gap-3.5 rounded-2xl px-5 py-3.5 text-left transition
						{d.primary
						? 'bg-gold-400 text-ink-950 shadow-[0_8px_30px_-8px] shadow-gold-400/70 hover:-translate-y-0.5 hover:bg-gold-300'
						: 'border border-white/10 bg-ink-800 text-cream-100 hover:-translate-y-0.5 hover:border-gold-400/40'}"
				>
					<iconify-icon icon={d.icon} class="text-3xl"></iconify-icon>
					<span>
						<span class="block text-sm font-semibold">
							Download for {d.os}
						</span>
						<span class="block text-xs {d.primary ? 'text-ink-950/70' : 'text-cream-500'}">
							{d.note}
						</span>
					</span>
					<iconify-icon
						icon="mdi:arrow-right"
						class="ms-auto text-lg opacity-0 transition group-hover:translate-x-0.5 group-hover:opacity-100"
					></iconify-icon>
				</a>
			{/each}
		</div>

		<p class="mt-5 text-xs text-cream-600">
			Latest build from
			<a
				href={SITE.releases}
				target="_blank"
				rel="noreferrer"
				class="underline decoration-cream-600/50 underline-offset-2 transition hover:text-gold-300"
				>GitHub Releases</a
			> · no account needed for local files
		</p>

		<!-- App screenshot -->
		<div class="relative mx-auto mt-16 max-w-4xl">
			<div
				class="absolute inset-x-12 -bottom-6 h-40 rounded-full bg-gold-400/15 blur-3xl"
				aria-hidden="true"
			></div>
			<div
				class="relative overflow-hidden rounded-2xl border border-white/10 shadow-2xl shadow-black/60"
			>
				<img
					src="{base}/images/ammini-playback-screenshot.webp"
					alt="Ammini playing a movie, with the playback bar and Telegram panel controls visible"
					class="block w-full"
					width="1313"
					height="650"
					loading="eager"
				/>
				<div
					class="pointer-events-none absolute inset-0 rounded-2xl ring-1 ring-white/10 ring-inset"
					aria-hidden="true"
				></div>
			</div>
		</div>
	</div>
</section>
