// Nets, DFM et mise en panneau dans Chromium sans fenêtre : surlignage d'un net depuis l'onglet Nets (longueur),
// vérification DFM et rapport PDF, panneau ouvert en aperçu et téléchargé en ZIP. Captures dans essais/fabrication/.
//
// Lancer après « npm run build » : node scripts/essai-fabrication.mjs
import { chromium } from "playwright";
import { PDFDocument } from "pdf-lib";
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import path from "node:path";

const racine = path.resolve(import.meta.dirname, "..");
const sortie = path.join(racine, "essais", "fabrication");
mkdirSync(sortie, { recursive: true });
const PORT = Number(process.env.PORT_ESSAI ?? 4197);
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
const etat = (page) => page.locator(".etat").textContent();

try {
  const page = await navigateur.newPage({ viewport: { width: 1440, height: 900 }, locale: "fr-FR", acceptDownloads: true });
  page.on("pageerror", (e) => erreurs.push(e.message));
  page.on("console", (m) => m.type() === "error" && erreurs.push(`console : ${m.text()}`));
  await page.goto(`http://localhost:${PORT}/`);
  await page.locator("h1").waitFor();
  await page.getByRole("button", { name: /La même en ODB\+\+/ }).click();
  await page.waitForFunction(() => /lue/.test(document.querySelector(".etat")?.textContent ?? ""), null, { timeout: 60000 });

  // Net : depuis l'onglet Nets
  await page.locator(".onglet", { hasText: "Nets" }).click();
  await page.locator(".contenu-onglet tbody tr", { hasText: "SDA" }).first().click();
  await page.locator(".nom-net").waitFor({ timeout: 30000 });
  const net = (await page.locator(".contenu-onglet").innerText()).replace(/\s+/g, " ");
  verifier(/Longueur des pistes \d/.test(net), `net : ${net}`);
  bilan.push(`Net SDA : ${net.match(/Longueur des pistes [^A-Z]*/)?.[0].trim()}`);
  await page.waitForTimeout(600);
  await page.screenshot({ path: path.join(sortie, "net.png") });
  await page.keyboard.press("Escape");

  // DFM
  await page.getByRole("button", { name: "Vérifications de fabricabilité (DFM)" }).click();
  await page.getByRole("button", { name: "Lancer la vérification" }).click();
  await page.waitForFunction(() => /DFM :/.test(document.querySelector(".etat")?.textContent ?? ""), null, { timeout: 120000 });
  bilan.push(`DFM carte d'exemple : ${(await etat(page)).trim()}`);
  const regle = page.locator(".regles-dfm .regle:not([disabled])").first();
  if (await regle.count()) {
    await regle.click();
    await page.locator(".defauts-dfm button").first().click();
    await page.waitForTimeout(600);
    await page.screenshot({ path: path.join(sortie, "dfm.png") });
  }
  const [rapport] = await Promise.all([page.waitForEvent("download", { timeout: 180000 }), page.getByRole("button", { name: "Rapport PDF" }).click()]);
  const fichier = path.join(sortie, "dfm.pdf");
  await rapport.saveAs(fichier);
  const pages = (await PDFDocument.load(readFileSync(fichier))).getPageCount();
  verifier(pages >= 3, `rapport DFM : ${pages} pages`);
  bilan.push(`Rapport DFM : ${pages} pages`);

  // Panneau
  await page.getByRole("button", { name: "Mettre la carte en panneau" }).click();
  await page.locator(".dialogue").waitFor();
  const taille = await page.locator(".taille-panneau").textContent();
  const [zip] = await Promise.all([page.waitForEvent("download", { timeout: 60000 }), page.getByRole("button", { name: /Télécharger \(ZIP\)/ }).click()]);
  await zip.saveAs(path.join(sortie, "panneau.zip"));
  verifier(readFileSync(path.join(sortie, "panneau.zip")).subarray(0, 2).toString() === "PK", "ZIP du panneau");
  await page.getByRole("button", { name: "Voir le panneau" }).click();
  await page.waitForFunction(() => /panneau/.test(document.querySelector(".pastille-doc")?.textContent ?? ""), null, { timeout: 60000 });
  await page.waitForTimeout(1500);
  const nb = await page.locator(".liste-couches .couche").count();
  verifier(nb >= 10, `panneau : ${nb} couches`);
  bilan.push(`Panneau : ${taille.trim()}, ${nb} couches relues`);
  await page.screenshot({ path: path.join(sortie, "panneau.png") });
  await page.getByRole("button", { name: "Voir la carte en 3D (V)" }).click();
  await page.locator(".legende-3d span").first().waitFor({ timeout: 60000 });
  await page.waitForTimeout(800);
  await page.screenshot({ path: path.join(sortie, "panneau-3d.png") });

  // DFM d'une vraie carte (IPC-2581 KiCad), si l'exemple a été téléchargé
  const feather = path.join(racine, "exemples-externes/ipc2581/royalblue54l-feather.xml");
  if (existsSync(feather)) {
    await page.goto(`http://localhost:${PORT}/`);
    await page.locator("h1").waitFor();
    const [ch] = await Promise.all([page.waitForEvent("filechooser"), page.getByRole("button", { name: /Choisir des fichiers/ }).click()]);
    await ch.setFiles(feather);
    await page.waitForFunction(() => /lue/.test(document.querySelector(".etat")?.textContent ?? ""), null, { timeout: 120000 });
    await page.getByRole("button", { name: "Vérifications de fabricabilité (DFM)" }).click();
    await page.getByRole("button", { name: "Lancer la vérification" }).click();
    await page.waitForFunction(() => /DFM :/.test(document.querySelector(".etat")?.textContent ?? ""), null, { timeout: 180000 });
    bilan.push(`DFM Feather (IPC-2581) : ${(await etat(page)).trim()}`);
    await page.locator(".regles-dfm .regle", { hasText: "Isolement" }).click();
    await page.waitForTimeout(800);
    await page.screenshot({ path: path.join(sortie, "dfm-feather.png") });
  }
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
