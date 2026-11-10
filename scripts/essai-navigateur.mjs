// Parcours complet dans Chromium (sans fenêtre) du site construit : accueil, carte d'exemple (dessus, dessous,
// sélection, mesure), ODB++ et ses tableaux, comparaison A/B (superposition, côte à côte, alternance, zones, rapport
// PDF), comparaison Gerber ↔ ODB++, téléphone. Captures dans captures/, fichiers produits dans essais/.
//
// Lancer après « npm run build » : node scripts/essai-navigateur.mjs
import { chromium } from "playwright";
import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { PDFDocument } from "pdf-lib";

const racine = path.resolve(import.meta.dirname, "..");
const captures = path.join(racine, "captures");
const essais = path.join(racine, "essais");
fs.mkdirSync(captures, { recursive: true });
fs.mkdirSync(essais, { recursive: true });
const PORT = Number(process.env.PORT_ESSAI) || 4191;
const bilan = [];
const erreurs = [];
const verifier = (condition, message) => {
  if (!condition) throw new Error(message);
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
  page.on("request", (r) => {
    if (!r.url().startsWith(`http://localhost:${PORT}`) && !r.url().startsWith("blob:") && !r.url().startsWith("data:")) erreurs.push(`${nom} : requête hors du site ${r.url()}`);
  });
};
const etat = (page) => page.locator(".etat").textContent();
const attendreEtat = async (page, motif, delai = 60000) => {
  await page.waitForFunction((m) => new RegExp(m).test(document.querySelector(".etat")?.textContent ?? ""), motif.source ?? motif, { timeout: delai });
};

try {
  /* Ordinateur */
  const contexte = await navigateur.newContext({ viewport: { width: 1440, height: 900 }, acceptDownloads: true, locale: "fr-FR" });
  const page = await contexte.newPage();
  surveiller(page, "ordinateur");
  await page.goto(`http://localhost:${PORT}/`);
  await page.getByText("Visionneuse et comparateur de fichiers de fabrication").waitFor();
  await page.waitForTimeout(500);
  await page.screenshot({ path: path.join(captures, "accueil.png") });

  // Carte d'exemple
  await page.getByRole("button", { name: /Carte d'exemple/ }).click();
  await attendreEtat(page, /couches lues/);
  await page.waitForTimeout(1500);
  await page.screenshot({ path: path.join(captures, "vue-dessus.png") });
  const nbCouches = await page.locator(".liste-couches .couche").count();
  verifier(nbCouches === 13, `carte d'exemple : ${nbCouches} couches au lieu de 13`);
  bilan.push(`Carte d'exemple : ${await etat(page)}`);
  // Les rôles viennent des attributs X2
  const libelles = await page.locator(".liste-couches .libelle").allTextContents();
  for (const l of ["Cuivre dessus", "Cuivre interne 2", "Cuivre interne 3", "Cuivre dessous", "Contour de la carte", "Perçage métallisé 1–4", "Perçage non métallisé 1–4", "Composants dessus"]) verifier(libelles.includes(l), `couche « ${l} » absente : ${libelles.join(", ")}`);

  // Sélection d'une pastille : U1 broche 2 (SDA), à (19,3 ; 23,27)
  const pointEcran = async (x, y) => page.evaluate(([x, y]) => {
    const v = document.querySelector(".visionneuse");
    const r = v.getBoundingClientRect();
    return { r: [r.left, r.top, r.width, r.height] };
  }, [x, y]);
  void pointEcran;
  // On passe par la vue : on centre sur le point, puis on clique au centre.
  const cliquerCarte = async (x, y) => {
    await page.evaluate(([x, y]) => window.__typon?.allerA?.(x, y, 3), [x, y]);
    await page.waitForTimeout(400);
    const boite = await page.locator(".visionneuse").boundingBox();
    await page.mouse.click(boite.x + boite.width / 2, boite.y + boite.height / 2);
    await page.waitForTimeout(600);
  };
  await cliquerCarte(22 - 2.7, 22 + 1.905 - 1.27);
  const inspecteur = await page.locator(".contenu-onglet").textContent();
  verifier(inspecteur.includes("SDA") && inspecteur.includes("U1"), `inspecteur : ${inspecteur.slice(0, 300)}`);
  await page.screenshot({ path: path.join(captures, "inspecteur.png") });
  bilan.push("Inspecteur : pastille U1-2 sur le net SDA");

  // Mesure entre deux points
  await page.keyboard.press("f");
  await page.keyboard.press("m");
  const vb = await page.locator(".visionneuse").boundingBox();
  await page.mouse.click(vb.x + vb.width * 0.3, vb.y + vb.height * 0.5);
  await page.mouse.click(vb.x + vb.width * 0.7, vb.y + vb.height * 0.5);
  await page.waitForTimeout(300);
  await page.screenshot({ path: path.join(captures, "mesure.png") });
  await page.keyboard.press("m");

  // Vue de dessous
  await page.getByRole("button", { name: /Vue de dessous/ }).click();
  await page.waitForTimeout(1200);
  await page.screenshot({ path: path.join(captures, "vue-dessous.png") });
  await page.getByRole("button", { name: /Vue de dessus/ }).click();

  // Onglets : carte, composants (X3)
  await page.getByRole("tab", { name: /Carte/ }).click();
  const resumeCarte = await page.locator(".contenu-onglet").textContent();
  verifier(resumeCarte.includes("64 mm × 42 mm") && resumeCarte.includes("ENIG"), `résumé : ${resumeCarte.slice(0, 400)}`);
  await page.getByRole("tab", { name: /Composants/ }).click();
  const nbComposants = await page.locator(".contenu-onglet tbody tr").count();
  verifier(nbComposants === 11, `composants X3 : ${nbComposants}`);
  await page.screenshot({ path: path.join(captures, "composants-x3.png") });

  // ODB++
  await page.locator(".marque").click();
  await page.getByRole("button", { name: /La même en ODB\+\+/ }).click();
  await attendreEtat(page, /couches lues/);
  await page.waitForTimeout(1200);
  await page.screenshot({ path: path.join(captures, "odb.png") });
  await page.getByRole("tab", { name: /Nomenclature/ }).click();
  const bom = await page.locator(".contenu-onglet").textContent();
  verifier(bom.includes("TD-R4K7") && bom.includes("R1, R2"), `nomenclature : ${bom.slice(0, 300)}`);
  await page.screenshot({ path: path.join(captures, "odb-nomenclature.png") });
  await page.getByRole("tab", { name: /Nets/ }).click();
  const nets = await page.locator(".contenu-onglet tbody tr").count();
  verifier(nets >= 8, `nets ODB++ : ${nets}`);
  bilan.push(`ODB++ : nomenclature et ${nets} nets`);

  // Comparaison A / B
  await page.locator(".marque").click();
  await page.getByRole("button", { name: /Comparer deux révisions/ }).click();
  await attendreEtat(page, /couches? avec des différences|Aucune différence/, 120000);
  await page.waitForTimeout(800);
  const message = await etat(page);
  bilan.push(`Comparaison A/B : ${message}`);
  const badges = await page.locator(".paire .badge").allTextContents();
  const paires = await page.locator(".paire .libelle").allTextContents();
  const differentes = paires.filter((_, i) => /zone/.test(badges[i]));
  for (const attendu of ["Cuivre dessus", "Sérigraphie dessus", "Perçage non métallisé 1–4", "Perçage métallisé 1–4"]) verifier(differentes.includes(attendu), `différence attendue sur « ${attendu} » : ${paires.map((p, i) => `${p}=${badges[i]}`).join(", ")}`);
  // C5 remplace R4 avec la même empreinte : masque et pâte inchangés.
  for (const pareil of ["Masque de soudure dessus", "Pâte à braser dessus", "Contour de la carte"]) verifier(!differentes.includes(pareil), `« ${pareil} » ne devrait pas changer`);
  await page.screenshot({ path: path.join(captures, "comparaison-superposition.png") });
  // Composants : R4 retiré, C5 ajouté
  const compAB = await page.locator(".composants-differences").textContent();
  verifier(compAB.includes("R4") && compAB.includes("C5") && compAB.includes("1 ajouté") && compAB.includes("1 retiré"), `composants A/B : ${compAB}`);
  // Zone suivante
  await page.keyboard.press("n");
  await page.waitForTimeout(600);
  await page.screenshot({ path: path.join(captures, "comparaison-zone.png") });
  await page.getByRole("button", { name: /Côte à côte/ }).click();
  await page.keyboard.press("f");
  await page.waitForTimeout(1000);
  await page.screenshot({ path: path.join(captures, "comparaison-cote-a-cote.png") });
  await page.getByRole("button", { name: /Superposition/ }).click();
  // Rapport PDF
  const [telechargement] = await Promise.all([page.waitForEvent("download", { timeout: 120000 }), page.getByRole("button", { name: "Rapport PDF" }).click()]);
  const cheminPdf = path.join(essais, "rapport.pdf");
  await telechargement.saveAs(cheminPdf);
  const pdf = await PDFDocument.load(fs.readFileSync(cheminPdf));
  verifier(pdf.getPageCount() >= 3, `rapport : ${pdf.getPageCount()} pages`);
  bilan.push(`Rapport PDF : ${pdf.getPageCount()} pages, ${Math.round(fs.statSync(cheminPdf).size / 1024)} Ko`);

  // Gerber ↔ ODB++ (même carte, même révision) : couches cuivre identiques attendues.
  await page.locator(".marque").click();
  await page.getByRole("button", { name: /Carte d'exemple/ }).click();
  await attendreEtat(page, /couches lues/);
  const [choix] = await Promise.all([page.waitForEvent("filechooser"), page.getByRole("button", { name: /Comparer avec une autre version/ }).click()]);
  await choix.setFiles(path.join(racine, "public/exemples/typon-demo-revA-odb.tgz"));
  await attendreEtat(page, /couches? avec des différences|Aucune différence/, 120000);
  const badges2 = await page.locator(".paire .badge").allTextContents();
  const paires2 = await page.locator(".paire .libelle").allTextContents();
  const resume2 = paires2.map((p, i) => `${p} : ${badges2[i]}`);
  bilan.push(`Gerber ↔ ODB++ : ${resume2.join(" ; ")}`);
  for (const c of ["Cuivre dessus", "Cuivre dessous", "Cuivre interne 2", "Cuivre interne 3"]) {
    const i = paires2.indexOf(c);
    verifier(i >= 0 && badges2[i] === "identique", `Gerber ↔ ODB++ : « ${c} » ${badges2[i]}`);
  }
  const iComp = paires2.indexOf("Composants dessus");
  verifier(badges2[iComp] === "voir la liste", `Gerber ↔ ODB++ : composants ${badges2[iComp]}`);
  await page.locator(".paire .nom-couche", { hasText: "Composants dessus" }).click();
  const listeComp = await page.locator(".composants-differences").textContent();
  verifier(listeComp.includes("11 composants sont identiques"), `composants Gerber ↔ ODB++ : ${listeComp}`);
  await page.screenshot({ path: path.join(captures, "comparaison-gerber-odb.png") });

  // Code-barres ODB++ (numéro de série de la carte d'exemple)
  await page.locator(".marque").click();
  await page.getByRole("button", { name: /La même en ODB\+\+/ }).click();
  await attendreEtat(page, /couches lues/);
  await page.evaluate(() => window.__typon.allerA(21.71, 6.5, 2));
  await page.waitForTimeout(800);
  const vb2 = await page.locator(".visionneuse").boundingBox();
  await page.mouse.click(vb2.x + vb2.width / 2, vb2.y + vb2.height / 2);
  await page.waitForTimeout(600);
  const infoBarre = await page.locator(".contenu-onglet").textContent();
  verifier(infoBarre.includes("code 39") && infoBarre.includes("TD-0001"), `code-barres : ${infoBarre.slice(0, 200)}`);
  await page.screenshot({ path: path.join(captures, "odb-code-barres.png") });
  bilan.push("Code-barres ODB++ : code 39 « TD-0001 » dessiné et sélectionnable");

  // Gerber RS-274D avec sa roue d'ouvertures (Eagle)
  await page.locator(".marque").click();
  {
    const [choix] = await Promise.all([page.waitForEvent("filechooser"), page.getByRole("button", { name: "Choisir des fichiers" }).click()]);
    await choix.setFiles(path.join(racine, "core/tests/donnees/rs274d-eagle.zip"));
  }
  await attendreEtat(page, /couches lues/);
  await page.waitForTimeout(800);
  await page.getByRole("tab", { name: /Fichiers/ }).click();
  await page.locator(".liste-fichiers button", { hasText: "Cuivre dessus" }).click();
  const details = await page.locator(".contenu-onglet").textContent();
  verifier(details.includes("Gerber RS-274D") && details.includes("typon-demo.whl") && details.includes("supposé"), `RS-274D : ${details.slice(0, 400)}`);
  await page.screenshot({ path: path.join(captures, "rs274d.png") });
  bilan.push("RS-274D : ouvertures lues dans la roue Eagle, format des coordonnées déduit");

  // IPC-2581 (export KiCad réel, s'il a été téléchargé)
  const ipc = path.join(racine, "exemples-externes/ipc2581/royalblue54l-feather.xml");
  if (fs.existsSync(ipc)) {
    await page.locator(".marque").click();
    const [choix] = await Promise.all([page.waitForEvent("filechooser"), page.getByRole("button", { name: "Choisir des fichiers" }).click()]);
    await choix.setFiles(ipc);
    await attendreEtat(page, /couches lues/, 120000);
    await page.waitForTimeout(2000);
    await page.getByRole("tab", { name: /Carte/ }).click();
    const carte = await page.locator(".contenu-onglet").textContent();
    verifier(carte.includes("Fichier IPC-2581") && carte.includes("KiCad"), `IPC-2581 : ${carte.slice(0, 300)}`);
    await page.screenshot({ path: path.join(captures, "ipc2581.png") });
    await page.getByRole("tab", { name: /Nomenclature/ }).click();
    const nbBom = await page.locator(".contenu-onglet tbody tr").count();
    verifier(nbBom > 10, `IPC-2581 : nomenclature ${nbBom}`);
    bilan.push(`IPC-2581 (KiCad 9, Feather) : ${await etat(page)} ; nomenclature de ${nbBom} lignes`);
  } else {
    bilan.push("IPC-2581 : exemple externe absent (node scripts/telecharger-exemples.mjs)");
  }
  await contexte.close();

  /* Téléphone */
  const mobile = await navigateur.newContext({ viewport: { width: 390, height: 844 }, deviceScaleFactor: 2, isMobile: true, hasTouch: true, locale: "fr-FR" });
  const tel = await mobile.newPage();
  surveiller(tel, "téléphone");
  await tel.goto(`http://localhost:${PORT}/`);
  await tel.getByText("Visionneuse et comparateur de fichiers de fabrication").waitFor();
  await tel.screenshot({ path: path.join(captures, "telephone-accueil.png") });
  await tel.getByRole("button", { name: /Carte d'exemple/ }).tap();
  await attendreEtat(tel, /couches lues/);
  await tel.waitForTimeout(1200);
  await tel.screenshot({ path: path.join(captures, "telephone-vue.png") });
  const debord = await tel.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  verifier(debord <= 1, `téléphone : la page déborde de ${debord} px`);
  await mobile.close();
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
console.log("\nTout est conforme. Captures dans captures/.");
