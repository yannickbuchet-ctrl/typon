// Langues de l'interface dans Chromium (sans fenêtre), sur le site construit : un navigateur en anglais (en-US) puis en
// français (fr-FR). Vérifie l'accueil, les exemples, les panneaux et la comparaison dans chaque langue (sans mot
// français oublié en anglais), le sélecteur FR/EN de la barre du haut et la mémorisation du choix. Captures dans
// essais/langues/ (dossier ignoré par git).
//
// Lancer après « npm run build » : node scripts/essai-langues.mjs   (port : variable PORT_ESSAI, 4196 par défaut)
import { chromium } from "playwright";
import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const racine = path.resolve(import.meta.dirname, "..");
const captures = path.join(racine, "essais", "langues");
fs.mkdirSync(captures, { recursive: true });
const PORT = Number(process.env.PORT_ESSAI) || 4196;
const bilan = [];
const erreurs = [];
/** Le texte contient tous ces mots, sans tenir compte de la casse (titres en capitales par la feuille de style). */
const contient = (texte, ...mots) => mots.every((m) => texte.toLowerCase().includes(m.toLowerCase()));
const verifier = (condition, message) => {
  if (!condition) throw new Error(message);
};

// Mots français qui ne doivent pas apparaître dans l'interface anglaise (sans tenir compte de la casse : certains titres
// sont en capitales par la feuille de style, et innerText les rend ainsi).
const FRANCAIS = /(?<!\p{L})(couches?|composants?|ouvrir|comparer|perçages?|cuivre|dessus|dessous|carte|fichiers?|sérigraphie|masque|métallisé|remarques|inspecteur|nomenclature|différences|aucune?|lecture|épaisseur|logiciel|contour|déposez|choisir|identique|seulement|zones?|rapport|unité|objet|broche|pastille|polarité|fonction|révision|étapes?|matrice|rôle|taille|raccourcis|échap|pouces?)(?!\p{L})/iu;
const motsFrancais = (texte) => {
  const trouves = new Set();
  for (const ligne of texte.split("\n")) {
    const m = ligne.match(new RegExp(FRANCAIS.source, "giu"));
    if (m) for (const x of m) trouves.add(`${x} (« ${ligne.trim().slice(0, 80)} »)`);
  }
  return [...trouves];
};

const serveur = spawn(process.execPath, [path.join(racine, "node_modules/vite/bin/vite.js"), "preview", "--port", String(PORT), "--strictPort"], { cwd: racine, stdio: "pipe" });
await new Promise((ok, ko) => {
  serveur.stdout.on("data", (d) => String(d).includes(String(PORT)) && ok());
  serveur.on("exit", ko);
  setTimeout(() => ko(new Error("serveur muet")), 20000);
});

const navigateur = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
const surveiller = (page, nom) => {
  page.on("pageerror", (e) => erreurs.push(`${nom} : ${e.message}`));
  page.on("console", (m) => m.type() === "error" && erreurs.push(`${nom} (console) : ${m.text()}`));
};
const attendreEtat = (page, motif, delai = 60000) => page.waitForFunction((m) => new RegExp(m).test(document.querySelector(".etat")?.textContent ?? ""), motif.source, { timeout: delai });
const texteApp = (page) => page.evaluate(() => document.querySelector(".app").innerText);
/** Vérifie qu'un texte d'interface anglaise ne contient aucun mot français connu. */
const sansFrancais = (texte, ou) => {
  const t = motsFrancais(texte);
  verifier(!t.length, `${ou} : texte français en anglais : ${t.join(" ; ")}`);
};
const onglet = async (page, nom) => {
  await page.getByRole("tab", { name: nom }).click();
  await page.waitForTimeout(200);
  return page.locator(".contenu-onglet").innerText();
};

try {
  /* Navigateur en anglais */
  const en = await navigateur.newContext({ viewport: { width: 1440, height: 900 }, acceptDownloads: true, locale: "en-US" });
  const page = await en.newPage();
  surveiller(page, "anglais");
  await page.goto(`http://localhost:${PORT}/`);
  await page.getByText("Viewer and comparator for printed circuit board manufacturing files").waitFor();
  verifier((await page.evaluate(() => document.documentElement.lang)) === "en", "anglais : <html lang> n'est pas « en »");
  const titre = await page.title();
  verifier(titre.startsWith("Typon: Viewer and comparator"), `anglais : titre de la page « ${titre} »`);
  sansFrancais(await texteApp(page), "accueil anglais");
  await page.screenshot({ path: path.join(captures, "en-accueil.png") });
  bilan.push(`Anglais : accueil et titre « ${titre} »`);

  // Carte d'exemple
  await page.getByRole("button", { name: /Sample board/ }).click();
  await attendreEtat(page, /layers read/);
  await page.waitForTimeout(1200);
  const libelles = await page.locator(".liste-couches .libelle").allTextContents();
  for (const l of ["Top copper", "Inner copper 2", "Bottom copper", "Board outline", "Plated drill 1–4", "Non-plated drill 1–4", "Top components", "Top solder mask", "Top silkscreen", "Top solder paste"]) verifier(libelles.includes(l), `anglais : couche « ${l} » absente : ${libelles.join(", ")}`);
  const textes = [await texteApp(page)];
  textes.push(await onglet(page, /Board/));
  textes.push(await onglet(page, /Components/));
  textes.push(await onglet(page, /Nets/));
  textes.push(await onglet(page, /Drills/));
  textes.push(await onglet(page, /Files/));
  await page.locator(".liste-fichiers button", { hasText: "Top copper" }).click();
  const details = await page.locator(".contenu-onglet").innerText();
  const detailsBruts = await page.locator(".contenu-onglet").textContent();
  verifier(detailsBruts.includes("File attributes (X2)") && detailsBruts.includes("Apertures") && /circle Ø0\.\d+ mm/.test(detailsBruts), `anglais : détails de la couche : ${detailsBruts.slice(0, 600)}`);
  // Les commentaires du fichier sont son contenu (en français dans la carte d'exemple) : ils restent tels quels.
  const commentaires = (await page.locator("pre.commentaires").count()) ? await page.locator("pre.commentaires").innerText() : "";
  textes.push(details.replace(commentaires, ""));
  // Inspecteur : pastille U1 broche 2 (SDA)
  await page.evaluate(() => window.__typon.allerA(22 - 2.7, 22 + 1.905 - 1.27, 3));
  await page.waitForTimeout(500);
  const vb = await page.locator(".visionneuse").boundingBox();
  await page.mouse.click(vb.x + vb.width / 2, vb.y + vb.height / 2);
  await page.waitForTimeout(700);
  const inspecteur = await page.locator(".contenu-onglet").innerText();
  verifier(contient(inspecteur, "Selected object", "SDA", "Pin"), `anglais : inspecteur : ${inspecteur.slice(0, 300)}`);
  textes.push(inspecteur);
  for (const t of textes) sansFrancais(t, "carte d'exemple en anglais");
  await page.screenshot({ path: path.join(captures, "en-carte.png") });
  bilan.push(`Anglais : carte d'exemple (${libelles.length} couches, onglets, détails, inspecteur)`);

  // ODB++
  await page.locator(".marque").click();
  await page.getByRole("button", { name: /The same board in ODB\+\+/ }).click();
  await attendreEtat(page, /layers read/);
  await page.waitForTimeout(1000);
  const odb = [await texteApp(page), await onglet(page, /Board/), await onglet(page, /BOM/), await onglet(page, /Drills/)];
  verifier(contient(odb[1], "Layer matrix", "Source software"), `anglais : ODB++ : ${odb[1].slice(0, 400)}`);
  for (const t of odb) sansFrancais(t, "ODB++ en anglais");
  await page.screenshot({ path: path.join(captures, "en-odb.png") });
  bilan.push("Anglais : ODB++ (matrice, nomenclature, perçages)");

  // Comparaison
  await page.locator(".marque").click();
  await page.getByRole("button", { name: /Compare two revisions/ }).click();
  await attendreEtat(page, /layers? with differences|No differences/, 120000);
  await page.waitForTimeout(800);
  const cmp = await texteApp(page);
  verifier(contient(cmp, "Compared layers", "added", "removed") && /\d+ areas?/.test(cmp), `anglais : comparaison : ${cmp.slice(0, 500)}`);
  sansFrancais(cmp, "comparaison en anglais");
  await page.screenshot({ path: path.join(captures, "en-comparaison.png") });
  const [telechargement] = await Promise.all([page.waitForEvent("download", { timeout: 120000 }), page.getByRole("button", { name: "PDF report" }).click()]);
  verifier(telechargement.suggestedFilename().startsWith("comparison "), `anglais : nom du rapport « ${telechargement.suggestedFilename()} »`);
  bilan.push(`Anglais : comparaison A/B (${await page.locator(".etat").textContent()}), rapport « ${telechargement.suggestedFilename()} »`);

  // Sélecteur : passage en français, la comparaison reste ouverte
  await page.locator("select.langue").selectOption("fr");
  await page.waitForTimeout(400);
  verifier((await page.evaluate(() => document.documentElement.lang)) === "fr", "sélecteur : <html lang> n'est pas « fr »");
  const enFrancais = await texteApp(page);
  verifier(contient(enFrancais, "Couches comparées", "Différences", "Cuivre dessus"), `sélecteur : comparaison en français : ${enFrancais.slice(0, 400)}`);
  verifier((await page.evaluate(() => localStorage.getItem("typon-langue"))) === "fr", "sélecteur : choix non mémorisé");
  await page.screenshot({ path: path.join(captures, "en-vers-fr-comparaison.png") });
  await page.reload();
  await page.getByText("Visionneuse et comparateur de fichiers de fabrication").waitFor();
  bilan.push("Sélecteur : anglais → français sans perdre la comparaison, choix mémorisé après rechargement");
  await en.close();

  /* Navigateur en français */
  const fr = await navigateur.newContext({ viewport: { width: 1440, height: 900 }, locale: "fr-FR" });
  const pfr = await fr.newPage();
  surveiller(pfr, "français");
  await pfr.goto(`http://localhost:${PORT}/`);
  await pfr.getByText("Visionneuse et comparateur de fichiers de fabrication").waitFor();
  verifier((await pfr.evaluate(() => document.documentElement.lang)) === "fr", "français : <html lang> n'est pas « fr »");
  const titreFr = await pfr.title();
  verifier(titreFr.startsWith("Typon — "), `français : titre de la page « ${titreFr} »`);
  await pfr.screenshot({ path: path.join(captures, "fr-accueil.png") });
  await pfr.getByRole("button", { name: /Carte d'exemple/ }).click();
  await attendreEtat(pfr, /couches lues/);
  await pfr.waitForTimeout(1000);
  const carteFr = await onglet(pfr, /Carte/);
  verifier(contient(carteFr, "Couches lues", "1,6 mm"), `français : résumé : ${carteFr.slice(0, 300)}`);
  const libellesFr = await pfr.locator(".liste-couches .libelle").allTextContents();
  verifier(libellesFr.includes("Perçage métallisé 1–4") && libellesFr.includes("Cuivre interne 3"), `français : couches : ${libellesFr.join(", ")}`);
  // Sélecteur : passage en anglais avec la carte ouverte
  await pfr.locator("select.langue").selectOption("en");
  await pfr.waitForTimeout(400);
  const nb = await pfr.locator(".liste-couches .couche").count();
  verifier(nb === 13, `sélecteur : ${nb} couches après le changement de langue`);
  const carteEn = await pfr.locator(".contenu-onglet").innerText();
  verifier(contient(carteEn, "Layers read", "1.6 mm", "Design rules (job file)"), `sélecteur : résumé en anglais : ${carteEn.slice(0, 400)}`);
  sansFrancais(await texteApp(pfr), "carte après passage en anglais");
  verifier((await pfr.title()).startsWith("Typon: "), "sélecteur : titre de la page non traduit");
  await pfr.screenshot({ path: path.join(captures, "fr-vers-en-carte.png") });
  await pfr.locator("select.langue").selectOption("fr");
  verifier((await pfr.title()) === titreFr, "sélecteur : titre français non rétabli");
  bilan.push("Français : accueil, carte d'exemple ; sélecteur français → anglais → français, carte gardée");
  await fr.close();
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
console.log(`\nTout est conforme. Captures dans ${path.relative(racine, captures)}/.`);
