/* Minimal service worker for the Zilean admin PWA.
 *
 * Caches the app shell on install (index.html, app.js, app.css,
 * manifest, icon) so the UI boots when the network is down. API calls
 * to /admin/api/* are always passed straight through — the whole point
 * of the admin portal is to see fresh data.
 */

const CACHE = "zilean-admin-v1";
const SHELL = [
  "./",
  "./index.html",
  "./app.js",
  "./app.css",
  "./manifest.webmanifest",
  "./icon.svg",
];

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE).then((c) => c.addAll(SHELL)));
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k)))
    )
  );
  self.clients.claim();
});

self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);

  // Never cache API traffic.
  if (url.pathname.includes("/admin/api/")) return;

  // GET-only cache-first shell.
  if (event.request.method !== "GET") return;

  event.respondWith(
    caches.match(event.request).then(
      (cached) =>
        cached ||
        fetch(event.request).then((response) => {
          if (!response.ok || response.type === "opaque") return response;
          const copy = response.clone();
          caches.open(CACHE).then((c) => c.put(event.request, copy));
          return response;
        })
    )
  );
});
