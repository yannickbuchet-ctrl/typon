/**
 * Rapport PDF d'une comparaison : synthèse par couche, vue d'ensemble de chaque couche modifiée avec ses zones
 * numérotées, puis gros plans des zones. Les images sont rendues par le moteur d'affichage (WebGL) sur le canevas.
 */
import { PDFDocument, rgb, StandardFonts, type PDFFont, type PDFPage } from "pdf-lib";
import { locale, t } from "../i18n";
import { traduireMoteur as tm } from "../i18n-moteur";
import type { Rendu, Vue } from "../rendu/rendu";
import { comparerComposants, type Paire } from "./comparaison";
import type { DocumentTypon } from "./document";
import { longueur, nombre, telecharger, type Unite } from "./dom";

interface Options {
  a: DocumentTypon;
  b: DocumentTypon;
  paires: Paire[];
  decalage: { dx: number; dy: number };
  tolerance: number;
  unite: Unite;
  rendu: Rendu;
}

/** Texte compatible avec les polices standard du PDF (WinAnsi). */
function propre(texte: string): string {
  texte = texte.replace(/Ω/g, "Ohm").replace(/μ/g, "µ");
  return texte.replace(/[  ]/g, " ").replace(/[«»]/g, (c) => c).replace(/[^\x20-\x7e -ÿ€–—…’‘“”•×]/g, "?");
}

const A4 = { l: 841.89, h: 595.28 }; // paysage
const MARGE = 36;
const ORANGE = rgb(0.91, 0.45, 0.23);
const GRIS = rgb(0.35, 0.39, 0.45);
const SOMBRE = rgb(0.08, 0.1, 0.13);

/** Rend une paire en superposition sur le canevas, cadrée sur `boite`, et renvoie l'image PNG. */
async function image(rendu: Rendu, p: Paire, decalage: { dx: number; dy: number }, boite: [number, number, number, number], largeur: number, hauteur: number): Promise<{ png: Uint8Array; vue: Vue }> {
  const canvas = rendu.canvas;
  const avant = { w: canvas.width, h: canvas.height };
  canvas.width = largeur;
  canvas.height = hauteur;
  const l = boite[2] - boite[0];
  const h = boite[3] - boite[1];
  const echelle = Math.min(largeur / l, hauteur / h);
  const vue: Vue = { cx: (boite[0] + boite[2]) / 2, cy: (boite[1] + boite[3]) / 2, echelle, miroir: false };
  const blanc: [number, number, number] = [1, 1, 1];
  rendu.dessiner([{ x: 0, y: 0, w: largeur, h: hauteur, vue, couches: [], difference: {
    a: p.a?.gpu ? [{ gpu: p.a.gpu, couleur: blanc, opacite: 1 }] : [],
    b: p.b?.gpu ? [{ gpu: p.b.gpu, couleur: blanc, opacite: 1, dx: decalage.dx, dy: decalage.dy }] : [],
  } }], [0.06, 0.08, 0.1]);
  const blob = await new Promise<Blob>((ok, ko) => canvas.toBlob((b) => (b ? ok(b) : ko(new Error(t("rapport.imageImpossible")))), "image/png"));
  canvas.width = avant.w;
  canvas.height = avant.h;
  return { png: new Uint8Array(await blob.arrayBuffer()), vue };
}

export async function rapportPdf(o: Options) {
  const pdf = await PDFDocument.create();
  pdf.setTitle(propre(t("rapport.titrePdf", { a: o.a.nom, b: o.b.nom })));
  pdf.setCreator("Typon");
  pdf.setProducer("Typon (pdf-lib)");
  const police = await pdf.embedFont(StandardFonts.Helvetica);
  const gras = await pdf.embedFont(StandardFonts.HelveticaBold);
  const date = new Date().toLocaleString(locale(), { dateStyle: "long", timeStyle: "short" });
  const ecrire = (page: PDFPage, t: string, x: number, y: number, taille = 10, f: PDFFont = police, couleur = SOMBRE) => page.drawText(propre(t), { x, y, size: taille, font: f, color: couleur });
  const entete = (page: PDFPage, titre: string) => {
    page.drawRectangle({ x: 0, y: A4.h - 6, width: A4.l, height: 6, color: ORANGE });
    ecrire(page, titre, MARGE, A4.h - MARGE - 8, 16, gras);
    ecrire(page, `Typon · ${date}`, A4.l - MARGE - 160, A4.h - MARGE - 8, 8, police, GRIS);
  };

  // Synthèse
  const synthese = pdf.addPage([A4.l, A4.h]);
  entete(synthese, t("rapport.titre"));
  let y = A4.h - MARGE - 40;
  for (const [k, v] of [
    [t("rapport.versionA"), t("rapport.couchesNom", { nom: o.a.nom, n: String(o.a.couches.length) })],
    [t("rapport.versionB"), t("rapport.couchesNom", { nom: o.b.nom, n: String(o.b.couches.length) })],
    [t("rapport.alignement"), o.decalage.dx || o.decalage.dy ? t("rapport.decalage", { dx: longueur(o.decalage.dx, o.unite, 3), dy: longueur(o.decalage.dy, o.unite, 3) }) : t("rapport.originesIdentiques")],
    [t("rapport.tolerance"), o.tolerance ? t("rapport.toleranceTexte", { t: longueur(o.tolerance, o.unite, 3) }) : t("rapport.aucune")],
  ]) {
    ecrire(synthese, k, MARGE, y, 10, gras);
    ecrire(synthese, v, MARGE + 150, y);
    y -= 16;
  }
  y -= 10;
  const colonnes = [MARGE, MARGE + 230, MARGE + 330, MARGE + 430, MARGE + 560, MARGE + 680];
  (["couche", "format", "resultat", "zones", "disparue", "apparue"] as const).forEach((c, i) => ecrire(synthese, t(`rapport.col.${c}`), colonnes[i], y, 9, gras, GRIS));
  y -= 6;
  synthese.drawLine({ start: { x: MARGE, y }, end: { x: A4.l - MARGE, y }, thickness: 0.6, color: GRIS });
  y -= 14;
  for (const p of o.paires) {
    if (y < MARGE + 20) break;
    const resultat = t(({ "a-analyser": "rapport.res.nonAnalysee", analyse: "rapport.res.nonAnalysee", identique: "rapport.res.identique", differences: "rapport.res.differences", "seulement-a": "rapport.res.seulementA", "seulement-b": "rapport.res.seulementB", representation: "rapport.res.representation" } as const)[p.etat]);
    const disparu = p.zones.reduce((s, z) => s + z.disparu, 0);
    const apparu = p.zones.reduce((s, z) => s + z.apparu, 0);
    const couleur = p.etat === "differences" ? ORANGE : p.etat === "identique" ? rgb(0.13, 0.55, 0.3) : GRIS;
    ecrire(synthese, tm(p.libelle).slice(0, 42), colonnes[0], y, 9);
    ecrire(synthese, `${p.a?.desc.format ?? "—"} / ${p.b?.desc.format ?? "—"}`, colonnes[1], y, 9, police, GRIS);
    ecrire(synthese, resultat, colonnes[2], y, 9, gras, couleur);
    ecrire(synthese, p.zones.length ? String(p.zones.length) : "", colonnes[3], y, 9);
    ecrire(synthese, disparu ? `${nombre(disparu, 3)} mm²` : "", colonnes[4], y, 9);
    ecrire(synthese, apparu ? `${nombre(apparu, 3)} mm²` : "", colonnes[5], y, 9);
    y -= 15;
  }
  ecrire(synthese, t("rapport.legende"), MARGE, MARGE, 8, police, GRIS);

  // Composants : ajoutés, retirés, modifiés
  const ca = o.a.description.meta.composants ?? [];
  const cb = o.b.description.meta.composants ?? [];
  if (ca.length && cb.length) {
    const d = comparerComposants(ca, cb, o.decalage);
    const page = pdf.addPage([A4.l, A4.h]);
    entete(page, t("rapport.composants"));
    let yc = A4.h - MARGE - 40;
    ecrire(page, t("rapport.composantsBilan", { ajoutes: String(d.ajoutes.length), retires: String(d.retires.length), modifies: String(d.modifies.length), inchanges: String(d.communs - d.modifies.length) }), MARGE, yc, 10, gras);
    yc -= 22;
    const lignes: [string, string, string][] = [
      ...d.retires.map((c) => ["retiré", c.reference, [c.valeur, c.boitier, c.piece].filter(Boolean).join(" · ")] as [string, string, string]),
      ...d.ajoutes.map((c) => ["ajouté", c.reference, [c.valeur, c.boitier, c.piece].filter(Boolean).join(" · ")] as [string, string, string]),
      ...d.modifies.map((m) => ["modifié", m.reference, m.changements.join(t("cmp.changement.separateur"))] as [string, string, string]),
    ];
    for (const [genre, ref, texte] of lignes) {
      if (yc < MARGE + 10) break;
      ecrire(page, t(`rapport.genre.${genre as "ajouté" | "retiré" | "modifié"}`), MARGE, yc, 9, gras, genre === "ajouté" ? rgb(0.1, 0.55, 0.25) : genre === "retiré" ? rgb(0.8, 0.15, 0.15) : ORANGE);
      ecrire(page, ref, MARGE + 70, yc, 9, gras);
      ecrire(page, texte.slice(0, 120), MARGE + 140, yc, 9);
      yc -= 14;
    }
    if (!lignes.length) ecrire(page, t("rapport.aucuneDifferenceComposants"), MARGE, yc, 10);
  }

  // Couches modifiées
  for (const p of o.paires.filter((q) => q.etat === "differences" && q.a?.gpu && q.b?.gpu)) {
    const page = pdf.addPage([A4.l, A4.h]);
    entete(page, propre(tm(p.libelle)));
    const ga = p.a!.gpu!.boite;
    const gb = p.b!.gpu!.boite;
    const boite: [number, number, number, number] = [Math.min(ga[0], gb[0] + o.decalage.dx), Math.min(ga[1], gb[1] + o.decalage.dy), Math.max(ga[2], gb[2] + o.decalage.dx), Math.max(ga[3], gb[3] + o.decalage.dy)];
    const m = Math.max(boite[2] - boite[0], boite[3] - boite[1]) * 0.03;
    const cadre: [number, number, number, number] = [boite[0] - m, boite[1] - m, boite[2] + m, boite[3] + m];
    const zoneImage = { x: MARGE, y: MARGE, w: 500, h: A4.h - 2 * MARGE - 40 };
    const echelleImg = Math.min(zoneImage.w / (cadre[2] - cadre[0]), zoneImage.h / (cadre[3] - cadre[1]));
    const wPt = (cadre[2] - cadre[0]) * echelleImg;
    const hPt = (cadre[3] - cadre[1]) * echelleImg;
    const { png } = await image(o.rendu, p, o.decalage, cadre, Math.round(wPt * 3), Math.round(hPt * 3));
    const img = await pdf.embedPng(png);
    const x0 = zoneImage.x;
    const y0 = zoneImage.y + zoneImage.h - hPt;
    page.drawImage(img, { x: x0, y: y0, width: wPt, height: hPt });
    const versPage = (x: number, yy: number) => [x0 + (x - cadre[0]) * echelleImg, y0 + (yy - cadre[1]) * echelleImg];
    p.zones.slice(0, 200).forEach((z, i) => {
      const [ax, ay] = versPage(z.x0, z.y0);
      const [bx, by] = versPage(z.x1, z.y1);
      page.drawRectangle({ x: ax - 2, y: ay - 2, width: Math.max(4, bx - ax + 4), height: Math.max(4, by - ay + 4), borderColor: rgb(1, 0.82, 0.4), borderWidth: 0.8 });
      ecrire(page, String(i + 1), ax - 2, by + 4, 6, gras, rgb(0.95, 0.7, 0.2));
    });
    // Tableau des zones
    let ty = A4.h - MARGE - 46;
    const tx = MARGE + 520;
    ecrire(page, t("rapport.zonesDifference", { n: p.zones.length }), tx, ty, 10, gras);
    ty -= 16;
    for (const [i, z] of p.zones.slice(0, 26).entries()) {
      const genre = z.disparu > 0 && z.apparu > 0 ? "modifié" : z.disparu > 0 ? "disparu" : "apparu";
      ecrire(page, `${i + 1}.`, tx, ty, 8, gras);
      ecrire(page, t(`cmp.genre.${genre}`), tx + 18, ty, 8, police, genre === "apparu" ? rgb(0.1, 0.55, 0.25) : genre === "disparu" ? rgb(0.8, 0.15, 0.15) : ORANGE);
      ecrire(page, `X ${longueur((z.x0 + z.x1) / 2, o.unite, 2)}  Y ${longueur((z.y0 + z.y1) / 2, o.unite, 2)}`, tx + 62, ty, 8);
      ty -= 15;
    }
    if (p.zones.length > 26) ecrire(page, t("rapport.autres", { n: String(p.zones.length - 26) }), tx, ty, 8, police, GRIS);

    // Gros plans : 8 par page
    const zones = p.zones.slice(0, 48);
    for (let k = 0; k < zones.length; k += 8) {
      const pg = pdf.addPage([A4.l, A4.h]);
      entete(pg, propre(t("rapport.grosPlans", { libelle: tm(p.libelle), de: String(k + 1), a: String(Math.min(k + 8, zones.length)) })));
      const cw = (A4.l - 2 * MARGE - 3 * 14) / 4;
      const ch = (A4.h - 2 * MARGE - 50 - 2 * 26) / 2;
      for (let j = 0; j < 8 && k + j < zones.length; j++) {
        const z = zones[k + j];
        const cx = MARGE + (j % 4) * (cw + 14);
        const cy = MARGE + (j < 4 ? ch + 26 : 0);
        const mz = Math.max(1, (z.x1 - z.x0) * 0.5, (z.y1 - z.y0) * 0.5);
        let gb2: [number, number, number, number] = [z.x0 - mz, z.y0 - mz, z.x1 + mz, z.y1 + mz];
        // Cadre aux proportions de la case
        const rl = gb2[2] - gb2[0];
        const rh = gb2[3] - gb2[1];
        if (rl / rh > cw / ch) {
          const nh = rl * (ch / cw);
          gb2 = [gb2[0], gb2[1] - (nh - rh) / 2, gb2[2], gb2[3] + (nh - rh) / 2];
        } else {
          const nl = rh * (cw / ch);
          gb2 = [gb2[0] - (nl - rl) / 2, gb2[1], gb2[2] + (nl - rl) / 2, gb2[3]];
        }
        const { png: png2 } = await image(o.rendu, p, o.decalage, gb2, Math.round(cw * 3), Math.round(ch * 3));
        const img2 = await pdf.embedPng(png2);
        pg.drawImage(img2, { x: cx, y: cy, width: cw, height: ch });
        const e2 = cw / (gb2[2] - gb2[0]);
        pg.drawRectangle({ x: cx + (z.x0 - gb2[0]) * e2 - 2, y: cy + (z.y0 - gb2[1]) * e2 - 2, width: (z.x1 - z.x0) * e2 + 4, height: (z.y1 - z.y0) * e2 + 4, borderColor: rgb(1, 0.82, 0.4), borderWidth: 0.8 });
        const genre = z.disparu > 0 && z.apparu > 0 ? "modifié" : z.disparu > 0 ? "disparu" : "apparu";
        ecrire(pg, t("rapport.zone", { i: String(k + j + 1), genre: t(`cmp.genre.${genre}`), x: longueur((z.x0 + z.x1) / 2, o.unite, 2), y: longueur((z.y0 + z.y1) / 2, o.unite, 2) }), cx, cy + ch + 6, 8, gras);
      }
    }
  }
  const octets = await pdf.save();
  const nom = t("rapport.fichier", { a: o.a.nom, b: o.b.nom }).replace(/[\\/:*?"<>|]/g, "_");
  telecharger(octets as BlobPart, nom, "application/pdf");
}
