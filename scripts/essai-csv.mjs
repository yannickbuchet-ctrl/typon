// Export CSV des tableaux, dans Chromium (sans fenêtre) : carte d'exemple en Gerber, la même en ODB++, comparaison
// des révisions A et B. Chaque bouton « CSV » est cliqué ; le fichier téléchargé doit commencer par le BOM UTF-8,
// utiliser le séparateur « ; » et les fins de ligne CRLF, avoir le même nombre de champs sur chaque ligne, et autant
// de lignes que le tableau affiché. Fichiers produits dans essais/csv/.
//
// Lancer après « npm run build » : node scripts/essai-csv.mjs (port : variable PORT_ESSAI, 4192 par défaut)
import { chromium } from "playwright";
import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const racine = path.resolve(import.meta.dirname, "..");
const sortie = path.join(racine, "essais", "csv");
fs.rmSync(sortie, { recursive: true, force: true });
fs.mkdirSync(sortie, { recursive: true });
const PORT = Number(process.env.PORT_ESSAI) || 4192;
const bilan = [];
const erreurs = [];
const verifier = (condition, message) => {
  if (!condition) throw new Error(message);
};

/** Lecture d'un CSV « ; » avec guillemets (champs multilignes compris). */
function lireCsv(texte) {
  const lignes = [];
  let ligne = [];
  let champ = "";
  let guillemets = false;
  for (let i = 0; i < texte.length; i++) {
    const c = texte[i];
    if (guillemets) {
      if (c === '"' && texte[i + 1] === '"') {
        champ += '"';
        i++;
      } else if (c === '"') guillemets = false;
      else champ += c;
    } else if (c === '"') guillemets = true;
    else if (c === ";") {
      ligne.push(champ);
      champ = "";
    } else if (c === "\r" && texte[i + 1] === "\n") {
      ligne.push(champ);
      lignes.push(ligne);
      ligne = [];
      champ = "";
      i++;
    } else champ += c;
  }
  verifier(!guillemets && champ === "" && ligne.length === 0, "CSV : fin de fichier inattendue (guillemet non fermé ou dernière ligne sans CRLF)");
  return lignes;
}

const serveur = spawn(process.execPath, [path.join(racine, "node_modules/vite/bin/vite.js"), "preview", "--port", String(PORT), "--strictPort"], { cwd: racine, stdio: "pipe" });
await new Promise((ok, ko) => {
  serveur.stdout.on("data", (d) => String(d).includes(String(PORT)) && ok());
  serveur.on("exit", ko);
  setTimeout(() => ko(new Error("serveur muet")), 20000);
});

const navigateur = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
const attendreEtat = async (page, motif, delai = 60000) => {
  await page.waitForFunction((m) => new RegExp(m).test(document.querySelector(".etat")?.textContent ?? ""), motif.source ?? motif, { timeout: delai });
};

/** Nombre de lignes du tableau (ou de la liste) que le bouton exporte, d'après l'affichage. */
const lignesAffichees = (bouton) => bouton.evaluate((b) => {
  const compter = (e) => {
    if (e.matches("tbody") || e.querySelector("tbody")) return (e.matches("tbody") ? e : e.querySelector("tbody")).children.length;
    if (e.matches("dl")) return e.querySelectorAll("dt").length;
    const liste = e.matches("ol.liste-zones, ul.liste-composants, ul.liste-fichiers, ul.paires") ? e : e.querySelector("ol.liste-zones, ul.liste-composants, ul.liste-fichiers, ul.paires");
    return liste ? liste.children.length : -1;
  };
  const tableau = b.closest(".tableau");
  if (tableau) return compter(tableau);
  for (let e = b.closest("h3, h4, .titre-panneau, .barre-tableau")?.nextElementSibling; e; e = e.nextElementSibling) {
    const n = compter(e);
    if (n >= 0) return n;
  }
  return -1;
});

let total = 0;
/** Clique tous les boutons CSV visibles de `zone`, et vérifie chaque fichier. Rend les fichiers lus par tableau. */
async function exporterTout(page, zone, contexte, prefixe) {
  const boutons = page.locator(`${zone} .bouton-csv`);
  const n = await boutons.count();
  const lus = {};
  for (let i = 0; i < n; i++) {
    const b = boutons.nth(i);
    const tableau = await b.getAttribute("data-tableau");
    const attendu = await lignesAffichees(b);
    const [telechargement] = await Promise.all([page.waitForEvent("download"), b.click()]);
    const nom = telechargement.suggestedFilename();
    const chemin = path.join(sortie, nom);
    await telechargement.saveAs(chemin);
    const octets = fs.readFileSync(chemin);
    const quoi = `${contexte}, « ${tableau} » (${nom})`;
    verifier(nom.endsWith(".csv") && nom.startsWith(prefixe) && !/[\\/:*?"<>|]/.test(nom), `${quoi} : nom de fichier inattendu`);
    verifier(octets[0] === 0xef && octets[1] === 0xbb && octets[2] === 0xbf, `${quoi} : pas de BOM UTF-8`);
    const texte = octets.subarray(3).toString("utf8");
    verifier(!/[^\r]\n/.test(texte), `${quoi} : fins de ligne autres que CRLF`);
    const lignes = lireCsv(texte);
    verifier(lignes.length >= 2, `${quoi} : ${lignes.length} ligne(s)`);
    verifier(lignes[0].length === 1 || texte.split("\r\n")[0].includes(";"), `${quoi} : en-tête sans séparateur « ; » : ${texte.split("\r\n")[0]}`);
    for (const [k, l] of lignes.entries()) verifier(l.length === lignes[0].length, `${quoi} : ligne ${k + 1} avec ${l.length} champs au lieu de ${lignes[0].length}`);
    verifier(attendu >= 0, `${quoi} : tableau affiché introuvable`);
    verifier(lignes.length - 1 === attendu, `${quoi} : ${lignes.length - 1} lignes exportées, ${attendu} affichées`);
    lus[tableau] = lignes;
    total++;
  }
  return lus;
}

/** Ouvre chaque onglet du panneau de droite et exporte ses tableaux. */
async function parcourirOnglets(page, contexte, prefixe) {
  const tous = {};
  const onglets = await page.getByRole("tab").allTextContents();
  for (const o of onglets) {
    if (/Inspecteur/.test(o)) continue;
    await page.getByRole("tab", { name: o.replace(/\d[\d\s ]*$/, "").trim() }).first().click();
    Object.assign(tous, await exporterTout(page, ".contenu-onglet", `${contexte}, onglet ${o}`, prefixe));
  }
  return tous;
}

try {
  const contexte = await navigateur.newContext({ viewport: { width: 1440, height: 900 }, acceptDownloads: true, locale: "fr-FR" });
  const page = await contexte.newPage();
  page.on("pageerror", (e) => erreurs.push(`page : ${e.message}`));
  await page.goto(`http://localhost:${PORT}/`);
  await page.getByText("Visionneuse et comparateur de fichiers de fabrication").waitFor();

  // Carte d'exemple en Gerber X2/X3
  await page.getByRole("button", { name: /Carte d'exemple/ }).click();
  await attendreEtat(page, /couches lues/);
  const gerber = await parcourirOnglets(page, "Gerber", "typon-demo-revA-");
  verifier(gerber.composants?.length === 12, `Gerber : composants ${gerber.composants?.length}`);
  verifier(gerber.composants[0].join("|") === "Repère|Valeur|Boîtier|Pièce|Côté|Rotation", `Gerber : en-tête des composants ${gerber.composants[0]}`);
  verifier(gerber.empilage && Object.keys(gerber).some((k) => k.startsWith("job ")) && gerber.fichiers?.length > 13, `Gerber : empilage ${!!gerber.empilage}, fichiers ${gerber.fichiers?.length}`);
  // Détails de couches : attributs X2, ouvertures, outils de perçage
  for (const couche of ["Cuivre dessus", "Perçage métallisé 1–4"]) {
    await page.getByRole("tab", { name: /Fichiers/ }).click();
    await page.locator(".liste-fichiers button", { hasText: couche }).first().click();
    const lus = await exporterTout(page, ".contenu-onglet", `Gerber, couche ${couche}`, "typon-demo-revA-");
    verifier(Object.keys(lus).length >= 2, `Gerber, couche ${couche} : ${Object.keys(lus).join(", ")}`);
  }
  const outils = Object.entries(gerber).find(([k]) => k.startsWith("perçages Perçage métallisé"))?.[1];
  verifier(outils && /^\d+,\d+ mm$/.test(outils[1][1]), `Gerber : diamètre d'outil à virgule décimale attendu : ${outils?.[1]}`);
  bilan.push(`Gerber : ${Object.keys(gerber).length} tableaux exportés (${Object.keys(gerber).join(", ")})`);

  // La même en ODB++
  await page.locator(".marque").click();
  await page.getByRole("button", { name: /La même en ODB\+\+/ }).click();
  await attendreEtat(page, /couches lues/);
  const odb = await parcourirOnglets(page, "ODB++", "typon-demo-revA-odb-");
  for (const t of ["composants", "nomenclature", "nets", "matrice des couches", "informations ODB++"]) verifier(odb[t], `ODB++ : tableau « ${t} » non exporté (${Object.keys(odb).join(", ")})`);
  verifier(odb.composants.length === 12, `ODB++ : composants ${odb.composants.length}`);
  verifier(odb.nomenclature.some((l) => l[3] === "R1, R2"), "ODB++ : nomenclature sans « R1, R2 »");
  bilan.push(`ODB++ : ${Object.keys(odb).length} tableaux exportés (${Object.keys(odb).join(", ")})`);

  // Comparaison des révisions A et B
  await page.locator(".marque").click();
  await page.getByRole("button", { name: /Comparer deux révisions/ }).click();
  await attendreEtat(page, /couches? avec des différences|Aucune différence/, 120000);
  const paires = await exporterTout(page, ".panneau.gauche", "Comparaison, couches comparées", "typon-demo-revA-typon-demo-revB-");
  verifier(paires["couches comparées"]?.some((l) => l[0] === "Cuivre dessus" && l[5] === "différences"), "Comparaison : « Cuivre dessus » sans différence dans le CSV des couches");
  await page.locator(".paire .nom-couche", { hasText: "Cuivre dessus" }).click();
  const diff = await exporterTout(page, ".contenu-onglet", "Comparaison, Cuivre dessus", "typon-demo-revA-typon-demo-revB-");
  const zones = diff["zones Cuivre dessus"];
  verifier(zones && zones.length > 1 && /^-?\d+(,\d+)?$/.test(zones[1][2]), `Comparaison : zones ${zones?.[1]}`);
  const composants = diff["différences de composants"];
  verifier(composants?.some((l) => l[0] === "retiré" && l[1] === "R4") && composants.some((l) => l[0] === "ajouté" && l[1] === "C5"), `Comparaison : composants ${composants}`);
  bilan.push(`Comparaison : ${Object.keys(paires).length + Object.keys(diff).length} tableaux exportés (${[...Object.keys(paires), ...Object.keys(diff)].join(", ")}), ${zones.length - 1} zones sur le cuivre dessus`);
  await contexte.close();
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
console.log(`\n${total} fichiers CSV conformes, dans essais/csv/.`);
