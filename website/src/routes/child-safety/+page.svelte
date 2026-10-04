<script lang="ts">
	import Nav from '$lib/components/Nav.svelte';
	import Footer from '$lib/components/Footer.svelte';
	import { SITE } from '$lib/site';
</script>

<svelte:head>
	<title>Child Safety Standards & Anti-CSAE Policy — Ammini</title>
	<meta
		name="description"
		content="Ammini's child safety standards and zero-tolerance anti-CSAE policy, including how to report abuse."
	/>
	<link rel="canonical" href="{SITE.url}/child-safety" />
	<meta name="robots" content="index, follow" />
</svelte:head>

<Nav />

<main class="px-6 pt-28 pb-24">
	<article
		class="mx-auto max-w-3xl rounded-3xl border border-white/5 bg-ink-900/60 px-6 py-12 sm:px-12 sm:py-16"
	>
		<header class="border-b border-white/10 pb-8">
			<p class="text-sm font-semibold tracking-widest text-gold-400 uppercase">Safety</p>
			<h1 class="mt-3 font-display text-3xl font-bold tracking-tight sm:text-4xl">
				Child Safety Standards &amp; Anti-CSAE Policy
			</h1>
			<dl class="mt-6 space-y-1 text-sm text-cream-300">
				<div class="flex gap-2">
					<dt class="font-semibold text-cream-100">App Name:</dt>
					<dd>Ammini</dd>
				</div>
				<div class="flex gap-2">
					<dt class="font-semibold text-cream-100">Developer:</dt>
					<dd>Anjaneya Sivan</dd>
				</div>
			</dl>
		</header>

		<div class="space-y-10 pt-12">
			<p class="text-cream-300">
				Ammini has a zero-tolerance stance toward Child Sexual Abuse Material (CSAM) and Child
				Sexual Abuse and Exploitation (CSAE). Ammini is a local client application designed to play
				video files from your Telegram account. Ammini does not host, store, index, or distribute
				media content on external servers.
			</p>

			<section>
				<h2 class="font-display text-xl font-semibold text-cream-100 sm:text-2xl">
					Reporting &amp; Abuse Handling
				</h2>
				<ul class="mt-4 space-y-4 text-cream-300">
					<li class="flex gap-3">
						<span class="mt-2 size-1.5 shrink-0 rounded-full bg-gold-400"></span>
						<span>
							<strong class="text-cream-100">Telegram Platform Content:</strong> Because all media
							accessible through Ammini originates directly from Telegram's cloud network, any
							illegal content hosted on Telegram should be reported directly to Telegram via their
							official abuse reporting channels (<a
								href="mailto:abuse@telegram.org"
								class="hover:text-gold-200 text-gold-300 underline decoration-gold-400/40 underline-offset-2 transition"
								>abuse@telegram.org</a
							>
							or
							<a
								href="mailto:stopCA@telegram.org"
								class="hover:text-gold-200 text-gold-300 underline decoration-gold-400/40 underline-offset-2 transition"
								>stopCA@telegram.org</a
							>) or through Telegram's in-app moderation mechanisms.
						</span>
					</li>
					<li class="flex gap-3">
						<span class="mt-2 size-1.5 shrink-0 rounded-full bg-gold-400"></span>
						<span>
							<strong class="text-cream-100">In-App Feedback &amp; Safety Contact:</strong> If you
							identify any safety concerns or child exploitation issues related to the Ammini
							application, please contact our child safety point of contact directly through the
							project's issue tracker at
							<a
								href={SITE.issues}
								target="_blank"
								rel="noreferrer"
								class="hover:text-gold-200 text-gold-300 underline decoration-gold-400/40 underline-offset-2 transition"
								>{SITE.issues}</a
							>.
						</span>
					</li>
				</ul>
			</section>
		</div>
	</article>
</main>

<Footer />
