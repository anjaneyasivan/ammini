<script lang="ts">
	const shortcuts = [
		{ keys: ['⌘', 'T'], label: 'Toggle Telegram panel' },
		{ keys: ['⌘', 'U'], label: 'Open URL' },
		{ keys: ['Space'], label: 'Play / pause' },
		{ keys: ['F'], label: 'Fullscreen' },
		{ keys: ['M'], label: 'Mute' },
		{ keys: ['←', '→'], label: 'Seek ±10 s' },
		{ keys: ['+'], label: 'Zoom in' },
		{ keys: ['−'], label: 'Zoom out' }
	];
</script>

<section id="shortcuts" class="scroll-mt-24 py-24">
	<div class="mx-auto max-w-6xl px-6">
		<div
			class="rounded-3xl border border-white/8 bg-gradient-to-b from-ink-900 to-ink-950 p-8 sm:p-12"
		>
			<div class="flex flex-wrap items-end justify-between gap-4">
				<div>
					<p class="text-sm font-semibold tracking-widest text-gold-400 uppercase">
						At your fingertips
					</p>
					<h2 class="mt-3 font-display text-3xl font-bold tracking-tight">
						Shortcuts that feel like home
					</h2>
				</div>
				<p class="text-sm text-cream-600">
					…plus a full playlist, recents menu, and per-file resume.
				</p>
			</div>

			<div class="mt-10 grid gap-x-10 gap-y-5 sm:grid-cols-2">
				{#each shortcuts as s (s.label)}
					<div class="flex items-center gap-4">
						<span class="flex gap-1.5">
							{#each s.keys as k (k)}
								<kbd
									class="flex h-8 min-w-8 items-center justify-center rounded-lg border border-b-2 border-white/15 border-b-white/25 bg-ink-800 px-2 font-sans text-sm font-semibold text-cream-300 shadow-sm"
								>
									{k}
								</kbd>
							{/each}
						</span>
						<span class="text-sm text-cream-500">{s.label}</span>
					</div>
				{/each}
			</div>
		</div>
	</div>
</section>
