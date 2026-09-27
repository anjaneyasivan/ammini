<script lang="ts">
	const features = [
		{
			icon: 'simple-icons:telegram',
			title: 'Telegram is your library',
			text: 'Sign in once and play any video from your chats — no downloads, no waiting. Your saved-videos folder becomes a streaming service.'
		},
		{
			icon: 'mdi:database-outline',
			title: 'Smart disk cache',
			text: 'Watched parts are cached in 512 KiB blocks on disk and survive restarts. Reopen a video and it picks up right where the bytes left off.'
		},
		{
			icon: 'mdi:speedometer',
			title: 'Instant seeking',
			text: 'Arrow keys and skip buttons seek by keyframe, so scrubbing stays snappy even over a streaming connection.'
		},
		{
			icon: 'mdi:restore',
			title: 'Resume everywhere',
			text: 'Quit whenever you like. Ammini remembers your position per file — local or Telegram — and offers to pick up where you left off.'
		},
		{
			icon: 'mdi:subtitles-outline',
			title: 'Tracks & subtitles',
			text: 'Switch audio and subtitle tracks on the fly from a clean dropdown, with human-friendly labels for every stream.'
		},
		{
			icon: 'mdi:keyboard-outline',
			title: 'Keyboard first',
			text: 'Space, F, M, arrows — everything is at your fingertips. The video owns the keys it needs and hands the rest back to you.'
		}
	];
</script>

<section id="features" class="scroll-mt-24 py-24">
	<div class="mx-auto max-w-6xl px-6">
		<p class="text-sm font-semibold tracking-widest text-gold-400 uppercase">Why Ammini</p>
		<h2
			class="mt-3 max-w-2xl font-display text-3xl font-bold tracking-tight text-balance sm:text-4xl"
		>
			A little player that takes video seriously
		</h2>
		<p class="mt-4 max-w-2xl text-cream-500">
			Built on mpv and egui, wrapped in a native macOS-style interface that stays out of your
			movie's way.
		</p>

		<div class="mt-12 grid gap-5 sm:grid-cols-2 lg:grid-cols-3">
			{#each features as f (f.title)}
				<div
					class="group rounded-2xl border border-white/8 bg-ink-900/80 p-6 transition hover:-translate-y-1 hover:border-gold-400/30 hover:bg-ink-800"
				>
					<div
						class="flex size-11 items-center justify-center rounded-xl border border-gold-400/20 bg-gold-400/10 text-xl text-gold-300 transition group-hover:bg-gold-400/20"
					>
						<iconify-icon icon={f.icon}></iconify-icon>
					</div>
					<h3 class="mt-5 font-display text-lg font-semibold">
						{f.title}
					</h3>
					<p class="mt-2 text-sm leading-relaxed text-cream-500">
						{f.text}
					</p>
				</div>
			{/each}
		</div>
	</div>
</section>
