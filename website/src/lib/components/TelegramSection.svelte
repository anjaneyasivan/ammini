<script lang="ts">
	import { base } from '$app/paths';

	const steps = [
		{
			title: 'Sign in to Telegram',
			text: 'One-time phone login from the sidebar. Your session stays on this machine.'
		},
		{
			title: 'Pick a chat',
			text: 'Browse your dialogs right inside Ammini — saved videos, channels, groups.'
		},
		{
			title: 'Press play',
			text: 'The video streams through a local proxy with byte-range support. It just plays.'
		}
	];
</script>

<section id="telegram" class="relative scroll-mt-24 overflow-hidden py-24">
	<div class="pointer-events-none absolute inset-0 -z-10" aria-hidden="true">
		<div
			class="absolute top-1/2 left-[-10%] h-96 w-96 -translate-y-1/2 rounded-full bg-[radial-gradient(closest-side,rgba(63,169,173,0.1),transparent)]"
		></div>
		<div
			class="absolute top-1/3 right-[-6%] h-96 w-96 rounded-full bg-[radial-gradient(closest-side,rgba(238,188,95,0.08),transparent)]"
		></div>
	</div>

	<div class="mx-auto grid max-w-6xl items-center gap-14 px-6 lg:grid-cols-2">
		<div>
			<p
				class="flex items-center gap-2 text-sm font-semibold tracking-widest text-lagoon-300 uppercase"
			>
				<iconify-icon icon="simple-icons:telegram"></iconify-icon>
				Telegram, unlocked
			</p>
			<h2 class="mt-3 font-display text-3xl font-bold tracking-tight text-balance sm:text-4xl">
				The movies people send you, playable in one click
			</h2>
			<p class="mt-4 leading-relaxed text-cream-500">
				Ammini streams Telegram videos through a private local server with full byte-range support,
				so mpv can seek like it would on a local file. A disk-backed block cache keeps what you've
				watched — reopen any video and it resumes from cache instead of re-downloading.
			</p>

			<ol class="mt-8 space-y-5">
				{#each steps as s, i (s.title)}
					<li class="flex gap-4">
						<span
							class="flex size-8 shrink-0 items-center justify-center rounded-full border border-gold-400/30 bg-gold-400/10 font-display text-sm font-bold text-gold-300"
						>
							{i + 1}
						</span>
						<span>
							<span class="block font-display font-semibold">{s.title}</span>
							<span class="block text-sm text-cream-500">{s.text}</span>
						</span>
					</li>
				{/each}
			</ol>

			<p
				class="mt-8 flex items-start gap-2.5 rounded-xl border border-white/8 bg-ink-900/80 p-4 text-sm text-cream-500"
			>
				<iconify-icon icon="mdi:shield-check-outline" class="mt-0.5 text-lg text-lagoon-300"
				></iconify-icon>
				<span>
					Everything runs locally — the proxy, the cache, the session. Signing out wipes the session
					and the cache directory, nothing lingers.
				</span>
			</p>
		</div>

		<div class="relative mx-auto w-72 sm:w-80">
			<div class="absolute inset-8 rounded-full bg-gold-400/20 blur-3xl" aria-hidden="true"></div>
			<img
				src="{base}/images/ammini-logo.webp"
				alt="The Ammini mascot — a smiling girl framed in a golden ring"
				class="relative w-full animate-float drop-shadow-2xl"
				width="1024"
				height="1024"
				loading="lazy"
			/>
		</div>
	</div>
</section>
