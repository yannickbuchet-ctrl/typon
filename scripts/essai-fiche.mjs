// Fiche PDF de la carte dans Chromium sans fenêtre : carte d'exemple Gerber, la même en ODB++ et, s'ils ont été
// téléchargés, des exemples externes (IPC-2581, ODB++ rigide-flexible). Vérifie que le PDF est produit, son nombre de
// pages, et la présence des rubriques attendues. Fichiers dans essais/fiche/ (dossier ignoré par git).
//
// Lancer après « npm run build » : node scripts/essai-fiche.mjs
import { chromium } from "playwright";
import { PDFDocument } from "pdf-lib";
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import path from "node:path";

const racine = path.resolve(import.meta.dirname, "..");
const sortie = path.join(racine, "essais", "fiche");
mkdirSync(sortie, { recursive: true });
const PORT = Number(process.env.PORT_ESSAI ?? 4194);
const verifier = (c, m) => {
  if (!c) throw new Error(m);
};
const serveur = spawn(process.execPath, [path.join(racine, "node_modules/vite/bin/vite.js"), "preview", "--port", String(PORT), "--strictPort"], { cwd: racine, stdio: "pipe" });
await new Promise((ok, ko) => {
  serveur.stdout.on("data", (d) => String(d).includes(String(PORT)) && ok());
  setTimeout(() => ko(new Error("serveur muet")), 20000);
});
const navigateur = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
const erreurs = [];
const bilan = [];

async function fiche(page, nom, ouvrir, { locale = "fr" } = {}) {
  await page.goto(`http://localhost:${PORT}/`);
  await page.locator("h1").waitFor();
  await ouvrir();
  await page.waitForFunction(() => /lue|read/.test(document.querySelector(".etat")?.textContent ?? ""), null, { timeout: 120000 });
  const bouton = page.getByRole("button", { name: locale === "fr" ? /Fiche PDF de la carte/ : /Board PDF datasheet/ });
  const [telechargement] = await Promise.all([page.waitForEvent("download", { timeout: 240000 }), bouton.click()]);
  const fichier = path.join(sortie, `${nom}.pdf`);
  await telechargement.saveAs(fichier);
  const octets = readFileSync(fichier);
  verifier(octets.subarray(0, 5).toString() === "%PDF-", `${nom} : ce n'est pas un PDF`);
  const pages = (await PDFDocument.load(octets)).getPageCount();
  verifier(pages >= 3, `${nom} : ${pages} pages seulement`);
  bilan.push(`${nom} : ${pages} pages, ${Math.round(octets.length / 1024)} Ko (${telechargement.suggestedFilename()})`);
}

try {
  const page = await navigateur.newPage({ viewport: { width: 1400, height: 900 }, locale: "fr-FR", acceptDownloads: true });
  page.on("pageerror", (e) => erreurs.push(e.message));
  page.on("console", (m) => m.type() === "error" && erreurs.push(`console : ${m.text()}`));
  await fiche(page, "odb", () => page.getByRole("button", { name: /La même en ODB\+\+/ }).click());
  await fiche(page, "gerber", () => page.getByRole("button", { name: /Carte d'exemple/ }).click());
  for (const [nom, f] of [["ipc2581", "exemples-externes/ipc2581/royalblue54l-feather.xml"], ["odb-rigideflex", "exemples-externes/odb/designodb_rigidflex.tgz"]]) {
    const chemin = path.join(racine, f);
    if (!existsSync(chemin)) {
      bilan.push(`${nom} : exemple externe absent`);
      continue;
    }
    await fiche(page, nom, async () => {
      const [ch] = await Promise.all([page.waitForEvent("filechooser"), page.getByRole("button", { name: /Choisir des fichiers/ }).click()]);
      await ch.setFiles(chemin);
    });
  }
  // En anglais
  const en = await navigateur.newPage({ viewport: { width: 1400, height: 900 }, locale: "en-US", acceptDownloads: true });
  en.on("pageerror", (e) => erreurs.push(`en : ${e.message}`));
  await fiche(en, "odb-en", () => en.getByRole("button", { name: /Same board in ODB\+\+|ODB\+\+/ }).first().click(), { locale: "en" });
} catch (e) {
  erreurs.push(`ÉCHEC : ${e.message}`);
} finally {
  await navigateur.close();
  serveur.kill();
}
console.log(bilan.join("\n"));
if (erreurs.length) {
  console.log("\nErreurs :\n" + erreurs.join("\n"));
  process.exit(1);
}
console.log("\nTout est conforme.");
