import { defineConfig, type Plugin } from "vitest/config";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";

/**
 * Politique de sécurité du site construit : aucune ressource ne vient d'un autre site et rien ne peut être envoyé
 * ailleurs (connect-src 'self'). Les fichiers de conception ouverts ne quittent jamais l'appareil.
 */
const SECURITE = [
  "default-src 'self'",
  "script-src 'self' 'wasm-unsafe-eval'",
  "worker-src 'self' blob:",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' data: blob:",
  "connect-src 'self'",
  "font-src 'self'",
  "object-src 'none'",
  "base-uri 'self'",
  "form-action 'none'",
].join("; ");

function politiqueDeSecurite(): Plugin {
  return {
    name: "politique-de-securite",
    apply: "build",
    transformIndexHtml: (html) => html.replace("<head>", `<head>\n    <meta http-equiv="Content-Security-Policy" content="${SECURITE}" />`),
  };
}

/**
 * Hors ligne : la liste des fichiers du site construit (hors-ligne.json) et un nom de cache propre à cette version
 * dans le service worker.
 */
function horsLigne(): Plugin {
  let sortie = "dist";
  return {
    name: "hors-ligne",
    apply: "build",
    configResolved: (config) => {
      sortie = path.resolve(config.root, config.build.outDir);
    },
    closeBundle() {
      const fichiers: string[] = [];
      const parcourir = (d: string) => {
        for (const n of fs.readdirSync(d)) {
          const p = path.join(d, n);
          if (fs.statSync(p).isDirectory()) parcourir(p);
          else fichiers.push(path.relative(sortie, p).replace(/\\/g, "/"));
        }
      };
      parcourir(sortie);
      const site = fichiers.filter((f) => !["sw.js", "hors-ligne.json", "index.html"].includes(f) && !f.startsWith("essai"));
      fs.writeFileSync(path.join(sortie, "hors-ligne.json"), JSON.stringify(site));
      const empreinte = createHash("sha256");
      for (const f of site.sort()) empreinte.update(f).update(fs.readFileSync(path.join(sortie, f)));
      const sw = path.join(sortie, "sw.js");
      fs.writeFileSync(sw, fs.readFileSync(sw, "utf8").replace("typon-dev", `typon-${empreinte.digest("hex").slice(0, 12)}`));
    },
  };
}

export default defineConfig({
  // Le site est statique : chemins relatifs, ou BASE fixée à la construction (GitHub Pages).
  base: process.env.BASE ?? "./",
  plugins: [politiqueDeSecurite(), horsLigne()],
  build: { target: "es2022", assetsInlineLimit: 0, chunkSizeWarningLimit: 2000 },
  worker: { format: "es" },
  test: { environment: "node", include: ["tests/**/*.test.ts"] },
});
