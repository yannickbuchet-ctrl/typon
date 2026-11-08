// Service worker de Typon : garde les fichiers du site (pages, scripts, moteur WebAssembly, exemples) pour qu'il
// fonctionne hors ligne. Les fichiers de conception ouverts sont lus sur l'appareil : ils ne passent jamais par lui.
// Le nom du cache est remplacé à chaque construction (vite.config.ts) : une nouvelle version efface l'ancienne.
const CACHE = "typon-dev";

self.addEventListener("install", (e) => {
  e.waitUntil((async () => {
    const cache = await caches.open(CACHE);
    await cache.addAll(["./", "./index.html"]);
    try {
      const liste = await (await fetch("./hors-ligne.json", { cache: "no-cache" })).json();
      await cache.addAll(liste);
    } catch {
      // Liste absente ou connexion coupée : les fichiers seront gardés au fil de leur utilisation.
    }
    await self.skipWaiting();
  })());
});

self.addEventListener("activate", (e) => {
  e.waitUntil(caches.keys().then((noms) => Promise.all(noms.filter((n) => n !== CACHE).map((n) => caches.delete(n)))).then(() => self.clients.claim()));
});

/** Un fichier du site ; une erreur passagère du serveur (5xx) ou du réseau est réessayée deux fois. */
async function telecharger(requete) {
  for (let essai = 0; ; essai++) {
    try {
      const r = await fetch(requete);
      if (r.status < 500 || essai >= 2) return r;
    } catch (erreur) {
      if (essai >= 2) throw erreur;
    }
    await new Promise((fin) => setTimeout(fin, 400 * (essai + 1)));
  }
}

self.addEventListener("fetch", (e) => {
  const requete = e.request;
  const url = new URL(requete.url);
  if (requete.method !== "GET" || url.origin !== self.location.origin) return;
  if (requete.mode === "navigate") {
    // La page : le réseau d'abord (mises à jour), la copie gardée hors ligne.
    e.respondWith(fetch(requete).then((r) => {
      const copie = r.clone();
      caches.open(CACHE).then((c) => c.put("./index.html", copie));
      return r;
    }).catch(() => caches.match("./index.html", { ignoreVary: true })));
    return;
  }
  e.respondWith(caches.match(requete, { ignoreVary: true }).then((gardee) => gardee || telecharger(requete).then((r) => {
    if (r.ok) {
      const copie = r.clone();
      caches.open(CACHE).then((c) => c.put(requete, copie));
    }
    return r;
  })));
});
