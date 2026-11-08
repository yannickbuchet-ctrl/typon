/**
 * Rapport PDF d'une comparaison : synthèse par couche, vue d'ensemble de chaque couche modifiée avec ses zones
 * numérotées, puis gros plans des zones. Les images sont rendues par le moteur d'affichage (WebGL) sur le canevas.
 */
import { PDFDocument, rgb, StandardFonts, type PDFFont, type PDFPage } from "pdf-lib";
import type { Rendu, Vue } from "../rendu/rendu";
import type { Paire } from "./comparaison";
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
function propre(t: string): string {
  return t.replace(/[  ]/g, " ").replace(/[«»]/g, (c) => c).replace(/[^\x20-\x7e -ÿ€–—…’‘“”•×]/g, "?");
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
  const blob = await new Promise<Blob>((ok, ko) => canvas.toBlob((b) => (b ? ok(b) : ko(new Error("Image impossible."))), "image/png"));
  canvas.width = avant.w;
  canvas.height = avant.h;
  return { png: new Uint8Array(await blob.arrayBuffer()), vue };
}

export async function rapportPdf(o: Options) {
  const pdf = await PDFDocument.create();
  pdf.setTitle(propre(`Comparaison ${o.a.nom} / ${o.b.nom}`));
  pdf.setCreator("Typon");
  pdf.setProducer("Typon (pdf-lib)");
  const police = await pdf.embedFont(StandardFonts.Helvetica);
  const gras = await pdf.embedFont(StandardFonts.HelveticaBold);
  const date = new Date().toLocaleString("fr-FR", { dateStyle: "long", timeStyle: "short" });
  const ecrire = (page: PDFPage, t: string, x: number, y: number, taille = 10, f: PDFFont = police, couleur = SOMBRE) => page.drawText(propre(t), { x, y, size: taille, font: f, color: couleur });
  const entete = (page: PDFPage, titre: string) => {
    page.drawRectangle({ x: 0, y: A4.h - 6, width: A4.l, height: 6, color: ORANGE });
    ecrire(page, titre, MARGE, A4.h - MARGE - 8, 16, gras);
    ecrire(page, `Typon · ${date}`, A4.l - MARGE - 160, A4.h - MARGE - 8, 8, police, GRIS);
  };

  // Synthèse
  const synthese = pdf.addPage([A4.l, A4.h]);
  entete(synthese, "Rapport de comparaison");
  let y = A4.h - MARGE - 40;
  for (const [k, v] of [
    ["Version A (référence)", `${o.a.nom}  (${o.a.couches.length} couches)`],
    ["Version B (nouvelle)", `${o.b.nom}  (${o.b.couches.length} couches)`],
    ["Alignement", o.decalage.dx || o.decalage.dy ? `B décalée de ${longueur(o.decalage.dx, o.unite, 3)} ; ${longueur(o.decalage.dy, o.unite, 3)}` : "origines identiques"],
    ["Tolérance", o.tolerance ? `écarts de moins de ${longueur(o.tolerance, o.unite, 3)} ignorés` : "aucune"],
  ]) {
    ecrire(synthese, k, MARGE, y, 10, gras);
    ecrire(synthese, v, MARGE + 150, y);
    y -= 16;
  }
  y -= 10;
  const colonnes = [MARGE, MARGE + 230, MARGE + 330, MARGE + 430, MARGE + 560, MARGE + 680];
  ["Couche", "Format A / B", "Résultat", "Zones", "Surface disparue", "Surface apparue"].forEach((t, i) => ecrire(synthese, t, colonnes[i], y, 9, gras, GRIS));
  y -= 6;
  synthese.drawLine({ start: { x: MARGE, y }, end: { x: A4.l - MARGE, y }, thickness: 0.6, color: GRIS });
  y -= 14;
  for (const p of o.paires) {
    if (y < MARGE + 20) break;
    const resultat = { "a-analyser": "non analysée", analyse: "non analysée", identique: "identique", differences: "différences", "seulement-a": "seulement dans A", "seulement-b": "seulement dans B" }[p.etat];
    const disparu = p.zones.reduce((s, z) => s + z.disparu, 0);
    const apparu = p.zones.reduce((s, z) => s + z.apparu, 0);
    const couleur = p.etat === "differences" ? ORANGE : p.etat === "identique" ? rgb(0.13, 0.55, 0.3) : GRIS;
    ecrire(synthese, p.libelle.slice(0, 42), colonnes[0], y, 9);
    ecrire(synthese, `${p.a?.desc.format ?? "—"} / ${p.b?.desc.format ?? "—"}`, colonnes[1], y, 9, police, GRIS);
    ecrire(synthese, resultat, colonnes[2], y, 9, gras, couleur);
    ecrire(synthese, p.zones.length ? String(p.zones.length) : "", colonnes[3], y, 9);
    ecrire(synthese, disparu ? `${nombre(disparu, 3)} mm²` : "", colonnes[4], y, 9);
    ecrire(synthese, apparu ? `${nombre(apparu, 3)} mm²` : "", colonnes[5], y, 9);
    y -= 15;
  }
  ecrire(synthese, "Rouge : présent seulement dans A (disparu). Vert : présent seulement dans B (apparu). Gris : commun aux deux versions.", MARGE, MARGE, 8, police, GRIS);

  // Couches modifiées
  for (const p of o.paires.filter((q) => q.etat === "differences" && q.a?.gpu && q.b?.gpu)) {
    const page = pdf.addPage([A4.l, A4.h]);
    entete(page, propre(p.libelle));
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
    ecrire(page, `${p.zones.length} zone${p.zones.length > 1 ? "s" : ""} de différence`, tx, ty, 10, gras);
    ty -= 16;
    for (const [i, z] of p.zones.slice(0, 26).entries()) {
      const genre = z.disparu > 0 && z.apparu > 0 ? "modifié" : z.disparu > 0 ? "disparu" : "apparu";
      ecrire(page, `${i + 1}.`, tx, ty, 8, gras);
      ecrire(page, genre, tx + 18, ty, 8, police, genre === "apparu" ? rgb(0.1, 0.55, 0.25) : genre === "disparu" ? rgb(0.8, 0.15, 0.15) : ORANGE);
      ecrire(page, `X ${longueur((z.x0 + z.x1) / 2, o.unite, 2)}  Y ${longueur((z.y0 + z.y1) / 2, o.unite, 2)}`, tx + 62, ty, 8);
      ty -= 15;
    }
    if (p.zones.length > 26) ecrire(page, `… et ${p.zones.length - 26} autres`, tx, ty, 8, police, GRIS);

    // Gros plans : 8 par page
    const zones = p.zones.slice(0, 48);
    for (let k = 0; k < zones.length; k += 8) {
      const pg = pdf.addPage([A4.l, A4.h]);
      entete(pg, propre(`${p.libelle} — gros plans ${k + 1} à ${Math.min(k + 8, zones.length)}`));
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
        ecrire(pg, `Zone ${k + j + 1} · ${genre} · X ${longueur((z.x0 + z.x1) / 2, o.unite, 2)}  Y ${longueur((z.y0 + z.y1) / 2, o.unite, 2)}`, cx, cy + ch + 6, 8, gras);
      }
    }
  }
  const octets = await pdf.save();
  const nom = `comparaison ${o.a.nom} - ${o.b.nom}.pdf`.replace(/[\\/:*?"<>|]/g, "_");
  telecharger(octets as BlobPart, nom, "application/pdf");
}
