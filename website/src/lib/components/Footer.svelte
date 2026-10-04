<script lang="ts">
	import { base } from '$app/paths';
	import { SITE } from '$lib/site';

	const year = new Date().getFullYear();
</script>

<footer class="border-t border-white/5 py-12">
	<div class="mx-auto flex max-w-6xl flex-col items-center justify-between gap-8 px-6 sm:flex-row">
		<div class="flex items-center gap-3">
			<img
				src="{base}/images/ammini-logo.webp"
				alt=""
				class="size-9 rounded-full"
				width="36"
				height="36"
				loading="lazy"
			/>
			<div>
				<p class="font-display font-semibold">Ammini</p>
				<p class="text-xs text-cream-600">
					© {year} Anjaneya · free & open source
				</p>
			</div>
		</div>

		<nav class="flex items-center gap-7 text-sm text-cream-500">
			<a href={SITE.repo} target="_blank" rel="noreferrer" class="transition hover:text-gold-300"
				>Source</a
			>
			<a
				href={SITE.releases}
				target="_blank"
				rel="noreferrer"
				class="transition hover:text-gold-300">Releases</a
			>
			<a href={SITE.issues} target="_blank" rel="noreferrer" class="transition hover:text-gold-300"
				>Issues</a
			>
			<a
				href="{SITE.repo}#readme"
				target="_blank"
				rel="noreferrer"
				class="transition hover:text-gold-300">Docs</a
			>
			<a href="{base}/privacy" class="transition hover:text-gold-300">Privacy</a>
		</nav>

		<p class="font-malayalam text-sm text-cream-600">അമ്മിണി</p>
	</div>
</footer>
