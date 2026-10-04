<script lang="ts">
	import Nav from '$lib/components/Nav.svelte';
	import Footer from '$lib/components/Footer.svelte';
	import { SITE } from '$lib/site';

	const updated = 'October 4, 2025';

	const permissions = [
		{
			name: 'Internet',
			purpose:
				"To connect to Telegram's servers (required for a Telegram client) and to the dial-code auto-fill service on the sign-in screen."
		},
		{
			name: 'Foreground service — media playback',
			purpose: 'To keep playing and control media from the notification while the screen is off.'
		},
		{
			name: 'Foreground service — data sync',
			purpose: 'To run your queued playlist downloads in the background.'
		},
		{
			name: 'Notifications',
			purpose: 'To show playback controls and download progress notifications.'
		}
	];
</script>

<svelte:head>
	<title>Privacy Policy — Ammini</title>
	<meta
		name="description"
		content="How Ammini handles information: no advertising, no analytics, no crash reporting. Almost all data stays on your device."
	/>
	<link rel="canonical" href="{SITE.url}/privacy" />
	<meta name="robots" content="index, follow" />
</svelte:head>

<Nav />

<main class="px-6 pt-28 pb-24">
	<article
		class="mx-auto max-w-3xl rounded-3xl border border-white/5 bg-ink-900/60 px-6 py-12 sm:px-12 sm:py-16"
	>
		<header class="border-b border-white/10 pb-8">
			<p class="text-sm font-semibold tracking-widest text-gold-400 uppercase">Legal</p>
			<h1 class="mt-3 font-display text-3xl font-bold tracking-tight sm:text-4xl">
				Privacy Policy — Ammini
			</h1>
			<p class="mt-4 text-sm text-cream-600">Effective date: {updated}</p>
			<p class="mt-6 text-cream-300">
				This privacy policy describes how the <strong class="text-cream-100">Ammini</strong> app ("the
				app", "we", "our") handles information when you use it. Ammini is an unofficial third-party video
				streaming player for Telegram: it lets you view and download videos from your own Telegram chats,
				signed in with your own Telegram account.
			</p>
			<p class="mt-4 text-cream-300">
				We have designed Ammini to be
				<strong class="text-cream-100">privacy-respecting</strong>: the app contains
				<strong class="text-cream-100"
					>no advertising, no analytics/tracking SDKs, and no crash-reporting service</strong
				>. Almost all data stays on your device and under your control.
			</p>
		</header>

		<div class="space-y-12 pt-12">
			<section>
				<h2 class="font-display text-xl font-semibold text-cream-100 sm:text-2xl">
					1. Information we do NOT collect
				</h2>
				<p class="mt-4 text-cream-300">Ammini does <strong class="text-cream-100">not</strong>:</p>
				<ul class="mt-4 space-y-2 text-cream-300">
					<li class="flex gap-3">
						<span class="mt-2 size-1.5 shrink-0 rounded-full bg-gold-400"></span>
						<span
							>collect, store, or transmit any personal information to us or any server we control;</span
						>
					</li>
					<li class="flex gap-3">
						<span class="mt-2 size-1.5 shrink-0 rounded-full bg-gold-400"></span>
						<span
							>use advertising, analytics, or telemetry SDKs (no Google Analytics, Firebase,
							Crashlytics, or similar);</span
						>
					</li>
					<li class="flex gap-3">
						<span class="mt-2 size-1.5 shrink-0 rounded-full bg-gold-400"></span>
						<span
							>send your chats, messages, or viewing history anywhere other than to Telegram's own
							servers (as required for the app to function) — and only for the chats you choose to
							open from your own account;</span
						>
					</li>
					<li class="flex gap-3">
						<span class="mt-2 size-1.5 shrink-0 rounded-full bg-gold-400"></span>
						<span>sell or share your data with any third party.</span>
					</li>
				</ul>
				<p class="mt-4 text-cream-300">
					There is <strong class="text-cream-100">no account registration with us</strong>, no
					backend we operate, and no server of ours receives any of your data.
				</p>
			</section>

			<section>
				<h2 class="font-display text-xl font-semibold text-cream-100 sm:text-2xl">
					2. Information the app processes
				</h2>

				<h3 class="mt-6 font-semibold text-cream-100">2.1 Your Telegram account and messages</h3>
				<p class="mt-3 text-cream-300">
					To function, Ammini connects to <strong class="text-cream-100">Telegram's servers</strong>
					using the Telegram messaging service's public API (via Telegram's own open-source "TDLib" library).
					This requires you to sign in with your
					<strong class="text-cream-100">phone number</strong> and Telegram login code (and, if enabled
					on your account, your two-step verification password).
				</p>
				<ul class="mt-4 space-y-2 text-cream-300">
					<li class="flex gap-3">
						<span class="mt-2 size-1.5 shrink-0 rounded-full bg-gold-400"></span>
						<span
							>Your phone number and Telegram login credentials are handled
							<strong class="text-cream-100"
								>directly by TDLib, which communicates only with Telegram's servers</strong
							>. We never see or store your credentials.</span
						>
					</li>
					<li class="flex gap-3">
						<span class="mt-2 size-1.5 shrink-0 rounded-full bg-gold-400"></span>
						<span
							>The app reads the
							<strong class="text-cream-100"
								>metadata of your chats and the videos within them</strong
							>
							— again, only by talking to Telegram's servers under your account, and only so it can display
							and play the videos you ask for.</span
						>
					</li>
					<li class="flex gap-3">
						<span class="mt-2 size-1.5 shrink-0 rounded-full bg-gold-400"></span>
						<span
							>This processing is
							<strong class="text-cream-100"
								>necessary to provide the app's core functionality</strong
							>
							(viewing and downloading videos from your Telegram chats). It is governed by Telegram's
							own terms of service and privacy policy; we do not retain a separate copy.</span
						>
					</li>
				</ul>

				<h3 class="mt-8 font-semibold text-cream-100">
					2.2 One-time country-codes lookup (dial-code auto-fill)
				</h3>
				<p class="mt-3 text-cream-300">
					On the <strong class="text-cream-100">sign-in screen only</strong>, the app makes a single
					HTTP request to the third-party service
					<code class="rounded bg-ink-800 px-1.5 py-0.5 text-sm text-gold-300">api.country.is</code>
					to guess your country's dialing code and pre-fill the phone field. That service receives your
					device's <strong class="text-cream-100">public IP address</strong> for this one request. This
					happens once, only on the sign-in screen, and is a convenience only — you can edit the dial
					code or remove the pre-filled value at any time before submitting. We do not log or store this
					lookup.
				</p>

				<h3 class="mt-8 font-semibold text-cream-100">2.3 Local playback bridge</h3>
				<p class="mt-3 text-cream-300">
					To play videos, the app runs a
					<strong class="text-cream-100">private, loopback-only HTTP server on your device</strong>
					(bound to
					<code class="rounded bg-ink-800 px-1.5 py-0.5 text-sm text-gold-300">127.0.0.1</code>,
					i.e. localhost only). This is used exclusively to stream video bytes from the app into the
					media player on the same device.
					<strong class="text-cream-100"
						>Nothing on this local server is reachable from the network</strong
					>; it never touches the internet and no external party can connect to it.
				</p>
			</section>

			<section>
				<h2 class="font-display text-xl font-semibold text-cream-100 sm:text-2xl">
					3. Data stored on your device
				</h2>
				<p class="mt-4 text-cream-300">
					All app data is stored locally in the app's private, sandboxed storage area, which other
					apps cannot read. This includes:
				</p>
				<ul class="mt-4 space-y-2 text-cream-300">
					<li class="flex gap-3">
						<span class="mt-2 size-1.5 shrink-0 rounded-full bg-gold-400"></span>
						<span
							><strong class="text-cream-100">Telegram data</strong> (via TDLib): your Telegram
							session, chat metadata, and downloaded video files, stored in the app's private
							<code class="rounded bg-ink-800 px-1.5 py-0.5 text-sm text-gold-300">files</code>
							directory.</span
						>
					</li>
					<li class="flex gap-3">
						<span class="mt-2 size-1.5 shrink-0 rounded-full bg-gold-400"></span>
						<span
							><strong class="text-cream-100">Your library</strong> (via a local Room database): your
							playlists, favorites, and download state.</span
						>
					</li>
					<li class="flex gap-3">
						<span class="mt-2 size-1.5 shrink-0 rounded-full bg-gold-400"></span>
						<span
							><strong class="text-cream-100">Preferences</strong> (via DataStore): your recently watched
							videos, per-video resume positions, and your theme preference.</span
						>
					</li>
				</ul>
				<p class="mt-4 text-cream-300">
					<strong class="text-cream-100">You can delete this data at any time</strong> using the
					<strong class="text-cream-100">Clear cache</strong> buttons in the app's
					<strong class="text-cream-100">Settings</strong> screen, or by uninstalling the app.
				</p>

				<h3 class="mt-8 font-semibold text-cream-100">Automated backup</h3>
				<p class="mt-3 text-cream-300">
					The app enables <strong class="text-cream-100">Android's standard app-data backup</strong>
					(<code class="rounded bg-ink-800 px-1.5 py-0.5 text-sm text-gold-300"
						>android:allowBackup="true"</code
					>). Depending on your device and Google account settings, some of the locally stored data
					above may be included in Android's automatic cloud backup. You can disable or manage this
					through your system settings. Downloaded video files are stored in the app's cache
					directory and may be cleared by the operating system.
				</p>
			</section>

			<section>
				<h2 class="font-display text-xl font-semibold text-cream-100 sm:text-2xl">
					4. Permissions
				</h2>
				<p class="mt-4 text-cream-300">
					The app requests only the permissions required for its core function:
				</p>
				<div class="mt-6 overflow-hidden rounded-2xl border border-white/10">
					<table class="w-full border-collapse text-left text-sm">
						<thead class="bg-ink-800/80 text-cream-100">
							<tr>
								<th class="px-4 py-3 font-semibold">Permission</th>
								<th class="px-4 py-3 font-semibold">Purpose</th>
							</tr>
						</thead>
						<tbody>
							{#each permissions as permission, i (permission.name)}
								<tr class={i % 2 === 1 ? 'bg-ink-800/40' : ''}>
									<td class="px-4 py-3 align-top font-medium text-cream-100">{permission.name}</td>
									<td class="px-4 py-3 align-top text-cream-300">{permission.purpose}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
				<p class="mt-4 text-cream-300">
					The app does <strong class="text-cream-100">not</strong> request access to your contacts, call
					log, photos, location, SMS, or microphone.
				</p>
			</section>

			<section>
				<h2 class="font-display text-xl font-semibold text-cream-100 sm:text-2xl">
					5. Children's privacy
				</h2>
				<p class="mt-4 text-cream-300">
					Ammini is not directed at children, and we do not knowingly process or collect information
					from children. The app is subject to Telegram's own terms and privacy practices for the
					underlying service.
				</p>
			</section>

			<section>
				<h2 class="font-display text-xl font-semibold text-cream-100 sm:text-2xl">
					6. Third-party services
				</h2>
				<p class="mt-4 text-cream-300">
					The app relies on the following external services, each with its own privacy practices:
				</p>
				<ul class="mt-4 space-y-2 text-cream-300">
					<li class="flex gap-3">
						<span class="mt-2 size-1.5 shrink-0 rounded-full bg-gold-400"></span>
						<span
							><strong class="text-cream-100">Telegram</strong> (via TDLib, the official Telegram
							library): handles sign-in and message/video data. See
							<a
								href="https://telegram.org/privacy"
								target="_blank"
								rel="noreferrer"
								class="hover:text-gold-200 text-gold-300 underline decoration-gold-400/40 underline-offset-2 transition"
								>Telegram's Privacy Policy</a
							>.</span
						>
					</li>
					<li class="flex gap-3">
						<span class="mt-2 size-1.5 shrink-0 rounded-full bg-gold-400"></span>
						<span
							><strong class="text-cream-100">api.country.is</strong>: a one-time IP-to-country
							lookup on the sign-in screen. See the service's own privacy policy for its handling of
							IP addresses.</span
						>
					</li>
				</ul>
				<p class="mt-4 text-cream-300">
					We do not control and are not responsible for these third parties' practices.
				</p>
			</section>

			<section>
				<h2 class="font-display text-xl font-semibold text-cream-100 sm:text-2xl">
					7. Data security and retention
				</h2>
				<p class="mt-4 text-cream-300">
					We do not operate our own servers, so we do not store, process, or retain your personal
					data anywhere we control. Data carried by TDLib between your device and Telegram's servers
					is protected by <strong class="text-cream-100">MTProto encryption</strong> (a Telegram security
					protocol), the same protection used by official Telegram clients.
				</p>
				<p class="mt-4 text-cream-300">
					Your locally stored data remains on your device until you clear it, sign out, or uninstall
					the app.
				</p>
			</section>

			<section>
				<h2 class="font-display text-xl font-semibold text-cream-100 sm:text-2xl">
					8. Your choices and controls
				</h2>
				<ul class="mt-4 space-y-2 text-cream-300">
					<li class="flex gap-3">
						<span class="mt-2 size-1.5 shrink-0 rounded-full bg-gold-400"></span>
						<span
							><strong class="text-cream-100">Sign out</strong> removes your Telegram session and locally
							stored app data from the device (available in Settings).</span
						>
					</li>
					<li class="flex gap-3">
						<span class="mt-2 size-1.5 shrink-0 rounded-full bg-gold-400"></span>
						<span
							><strong class="text-cream-100">Clear cache</strong> removes downloaded files and cached
							data (available in Settings).</span
						>
					</li>
					<li class="flex gap-3">
						<span class="mt-2 size-1.5 shrink-0 rounded-full bg-gold-400"></span>
						<span
							><strong class="text-cream-100">Uninstall</strong> the app to remove all locally stored
							data.</span
						>
					</li>
				</ul>
			</section>

			<section>
				<h2 class="font-display text-xl font-semibold text-cream-100 sm:text-2xl">
					9. Changes to this policy
				</h2>
				<p class="mt-4 text-cream-300">
					We may update this policy from time to time to reflect changes in the app's functionality.
					Any changes will be reflected by updating the "Effective date" at the top of this page.
					Material changes will be noted in the app's changelog.
				</p>
			</section>

			<section>
				<h2 class="font-display text-xl font-semibold text-cream-100 sm:text-2xl">10. Contact</h2>
				<p class="mt-4 text-cream-300">
					If you have questions about this privacy policy or how Ammini handles your data, please
					contact us at the developer's contact address listed on the Google Play listing for this
					app.
				</p>
			</section>

			<p class="border-t border-white/10 pt-8 text-sm text-cream-600 italic">
				This privacy policy describes Ammini as of its current version. Ammini is not affiliated
				with, endorsed by, or sponsored by Telegram or Telegram Messenger LLP.
			</p>
		</div>
	</article>
</main>

<Footer />
