// Minimal offline static-asset cache, shared by every page (homepage + each game's own
// manifest-scoped app — see xtask::sw_register_bridge). Network-first, falling back to
// cache only when the network fetch fails outright (offline): none of these assets are
// content-hashed (a game's .wasm keeps the same filename across every deploy), so
// stale-while-revalidate — this worker's original strategy — would keep serving an
// infrequent visitor's old cached copy indefinitely; it only "heals" on that visitor's
// *next* load after a deploy, which for a rarely-revisited page can be arbitrarily far in
// the future. Network-first still isn't a full redownload on every visit — the fetch()
// below goes through the browser's own HTTP cache (Cache-Control/ETag), which GitHub
// Pages sets a real max-age on — it just means "online" always means "current", and the
// Cache Storage entries this worker maintains are purely a last-resort fallback for
// genuinely offline revisits. No precache list — assets fill in as they're visited, which
// is enough for that without hand-maintaining a manifest of every game's .wasm/preview/
// font files.
const CACHE = "hcg-v2";

self.addEventListener("install", () => {
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener("fetch", (event) => {
  if (event.request.method !== "GET") return;
  const url = new URL(event.request.url);
  if (url.origin !== location.origin) return;

  event.respondWith(
    caches.open(CACHE).then((cache) =>
      fetch(event.request)
        .then((response) => {
          if (response.ok) cache.put(event.request, response.clone());
          return response;
        })
        .catch(() => cache.match(event.request))
    )
  );
});
