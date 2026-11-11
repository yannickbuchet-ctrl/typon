/**
 * Fiche PDF d'une carte : page de garde avec la carte en 3D et ses caractéristiques, vues de dessus et de dessous,
 * puis les tableaux (empilage, couches, perçages, composants, nomenclature, nets) et les remarques de lecture.
 * Les images sont rendues par le moteur d'affichage : la vue 2D (canevas de la visionneuse) et une vue 3D hors écran.
 */
import { PDFDocument, rgb, StandardFonts, type PDFFont, type PDFImage, type PDFPage } from "pdf-lib";
import { langue, locale } from "../i18n";
import { traduireMoteur as tm } from "../i18n-moteur";
import { couleurRgb, type CoucheAffichee, type Rendu, type Vue } from "../rendu/rendu";
import type { DocumentTypon, EtatCouche } from "./document";
import { longueur, nombre, taille, telecharger, type Unite } from "./dom";
import { Vue3D } from "./vue3d";

export interface Options {
  doc: DocumentTypon;
  rendu: Rendu;
  unite: Unite;
}

/** Texte selon la langue de l'interface. */
const L = (fr: string, en: string) => (langue() === "en" ? en : fr);

/** Texte compatible avec les polices standard du PDF (WinAnsi). */
function propre(texte: string): string {
  return String(texte)
    .replace(/[  ]/g, " ")
    .replace(/Ω/g, "Ohm")
    .replace(/[→]/g, "->")
    .replace(/[≤]/g, "<=")
    .replace(/[≥]/g, ">=")
    .replace(/[^\x20-\x7e -ÿ€–—…’‘“”•×]/g, "?");
}

const PAGE = { l: 595.28, h: 841.89 }; // A4 portrait
const MARGE = 40;
const ORANGE = rgb(0.91, 0.45, 0.23);
const GRIS = rgb(0.4, 0.43, 0.48);
const CLAIR = rgb(0.93, 0.94, 0.95);
const SOMBRE = rgb(0.08, 0.1, 0.13);

/** Couches d'un côté, dans l'ordre de dessin de la visionneuse (du bas vers le haut). */
function couchesDuCote(doc: DocumentTypon, cote: "dessus" | "dessous"): EtatCouche[] {
  const commun = (c: EtatCouche) => ["percage", "fraisage", "contour"].includes(c.desc.role.genre);
  const choisies = doc.couches.filter((c) => {
    const r = c.desc.role;
    if (c.desc.objets === 0) return false;
    if (commun(c)) return true;
    return r.cote === cote && ["cuivre", "masque", "serigraphie"].includes(r.genre);
  });
  const ordre = [...choisies].sort((a, b) => {
    if (cote === "dessous" && commun(a) === commun(b)) return a.desc.role.rang - b.desc.role.rang;
    return b.desc.role.rang - a.desc.role.rang;
  });
  return ordre.reverse();
}

/** Rend un côté de la carte en 2D (couleurs de la visionneuse) et renvoie l'image PNG. */
export async function vue2d(o: Options, cote: "dessus" | "dessous", boite: [number, number, number, number], largeur: number, hauteur: number): Promise<Uint8Array> {
  const couches = couchesDuCote(o.doc, cote);
  await Promise.all(couches.map((c) => o.doc.gpu(o.rendu, c)));
  const affichees: CoucheAffichee[] = couches.filter((c) => c.gpu).map((c) => ({ gpu: c.gpu!, couleur: couleurRgb(c.couleur), opacite: c.opacite }));
  const canvas = o.rendu.canvas;
  const avant = { w: canvas.width, h: canvas.height };
  canvas.width = largeur;
  canvas.height = hauteur;
  const echelle = Math.min(largeur / (boite[2] - boite[0]), hauteur / (boite[3] - boite[1]));
  const vue: Vue = { cx: (boite[0] + boite[2]) / 2, cy: (boite[1] + boite[3]) / 2, echelle, miroir: cote === "dessous" };
  o.rendu.dessiner([{ x: 0, y: 0, w: largeur, h: hauteur, vue, couches: affichees }], [0.06, 0.08, 0.1]);
  const blob = await new Promise<Blob>((ok, ko) => canvas.toBlob((b) => (b ? ok(b) : ko(new Error("PNG"))), "image/png"));
  canvas.width = avant.w;
  canvas.height = avant.h;
  return new Uint8Array(await blob.arrayBuffer());
}

export async function fichePdf(o: Options) {
  const { doc, unite } = o;
  const d = doc.description;
  const meta = d.meta;
  const pdf = await PDFDocument.create();
  pdf.setTitle(propre(L(`Fiche de la carte ${doc.nom}`, `Board datasheet ${doc.nom}`)));
  pdf.setCreator("Typon");
  pdf.setProducer("Typon (pdf-lib)");
  const police = await pdf.embedFont(StandardFonts.Helvetica);
  const gras = await pdf.embedFont(StandardFonts.HelveticaBold);
  const date = new Date().toLocaleString(locale(), { dateStyle: "long", timeStyle: "short" });

  const ecrire = (page: PDFPage, texte: string, x: number, y: number, t = 9, f: PDFFont = police, couleur = SOMBRE) => page.drawText(propre(texte), { x, y, size: t, font: f, color: couleur });
  /** Coupe un texte à une largeur (points), avec « … ». */
  const couper = (texte: string, largeur: number, t: number, f: PDFFont) => {
    let s = propre(texte);
    if (f.widthOfTextAtSize(s, t) <= largeur) return s;
    while (s.length > 1 && f.widthOfTextAtSize(`${s}…`, t) > largeur) s = s.slice(0, -1);
    return `${s}…`;
  };

  let numero = 0;
  const pages: PDFPage[] = [];
  const nouvellePage = (titre: string) => {
    const page = pdf.addPage([PAGE.l, PAGE.h]);
    pages.push(page);
    numero++;
    page.drawRectangle({ x: 0, y: PAGE.h - 6, width: PAGE.l, height: 6, color: ORANGE });
    ecrire(page, titre, MARGE, PAGE.h - MARGE - 6, 15, gras);
    ecrire(page, couper(doc.nom, 200, 8, police), PAGE.l - MARGE - police.widthOfTextAtSize(couper(doc.nom, 200, 8, police), 8), PAGE.h - MARGE - 4, 8, police, GRIS);
    return page;
  };

  // --- Caractéristiques ---------------------------------------------------------------------------------------
  const b = doc.boite;
  const formats = [...new Set(d.couches.map((c) => c.meta.format ?? c.format))].join(", ");
  const logiciel = d.couches.flatMap((c) => c.meta.attributs ?? []).find(([k]) => k === "Logiciel" || k === "Logiciel d'origine")?.[1]
    ?? meta.odb?.info.find(([k]) => k === "Logiciel d'origine")?.[1];
  const dateCreation = d.couches.flatMap((c) => c.meta.attributs ?? []).find(([k]) => k === "Date de création")?.[1]
    ?? meta.odb?.info.find(([k]) => k === "Date de création")?.[1];
  const composants = meta.composants ?? [];
  const dessous = composants.filter((c) => /dessous|bot/i.test(c.cote)).length;
  // Perçages : outils des fichiers Excellon et des couches de perçage ODB++ / IPC-2581.
  const outils: { couche: string; outil: string; genre: string; diametre: number; trous: number }[] = [];
  for (const c of d.couches) for (const t of c.meta.outils ?? []) outils.push({ couche: tm(c.nom), outil: `T${String(t.numero).padStart(2, "0")}`, genre: t.fraisages ? L(`${t.fraisages} fraisage(s)`, `${t.fraisages} slot(s)`) : "", diametre: t.diametre, trous: t.trous });
  if (!outils.length) for (const p of meta.odb?.outils ?? []) for (const t of p.outils) outils.push({ couche: `${p.couche}${p.debut ? ` (${p.debut} -> ${p.fin})` : ""}`, outil: String(t.numero), genre: tm(t.genre), diametre: t.fini || t.percage, trous: t.trous });
  const trous = outils.reduce((s, t) => s + t.trous, 0);
  // Les diamètres nuls ou presque (vias sans cote dans certains exports) ne comptent pas.
  const diametres = outils.filter((t) => t.diametre >= 0.05 && t.trous > 0).map((t) => t.diametre);
  const caracteristiques: [string, string | number | null | undefined][] = [
    [L("Nom", "Name"), doc.nom],
    [L("Dimensions", "Size"), b ? `${longueur(b[2] - b[0], unite, 2)} × ${longueur(b[3] - b[1], unite, 2)}` : null],
    [L("Épaisseur", "Thickness"), meta.epaisseur ? `${longueur(meta.epaisseur.valeur, unite, 3)} (${meta.epaisseur.source === "fichier de job" ? L("fichier de job", "job file") : meta.epaisseur.source})` : L("non indiquée", "not stated")],
    [L("Finition", "Finish"), meta.finition],
    [L("Couches de cuivre", "Copper layers"), meta.couchesCuivre || null],
    [L("Couches lues", "Layers read"), d.couches.length],
    [L("Formats", "Formats"), formats],
    [L("Logiciel d'origine", "Source software"), logiciel],
    [L("Date de création", "Creation date"), dateCreation],
    [L("Composants", "Components"), composants.length ? (dessous ? L(`${composants.length} (${composants.length - dessous} dessus, ${dessous} dessous)`, `${composants.length} (${composants.length - dessous} top, ${dessous} bottom)`) : composants.length) : null],
    [L("Lignes de nomenclature", "BOM lines"), meta.nomenclature?.length || null],
    [L("Nets", "Nets"), meta.nets?.length || meta.netlist?.nets || null],
    [L("Trous", "Holes"), trous || null],
    [L("Plus petit perçage", "Smallest drill"), diametres.length ? longueur(Math.min(...diametres), unite, 3) : null],
    [L("Plus gros perçage", "Largest drill"), diametres.length ? longueur(Math.max(...diametres), unite, 3) : null],
    [L("Taille des fichiers", "Files size"), taille(doc.taille)],
  ];

  // --- Page de garde : 3D et caractéristiques ------------------------------------------------------------------
  const garde = nouvellePage(L("Fiche de la carte", "Board datasheet"));
  ecrire(garde, couper(doc.nom, PAGE.l - 2 * MARGE, 12, gras), MARGE, PAGE.h - MARGE - 26, 12, gras, ORANGE);
  ecrire(garde, L(`Établie par Typon le ${date}`, `Generated by Typon on ${date}`), MARGE, PAGE.h - MARGE - 40, 8, police, GRIS);
  const zone3d = { x: MARGE, w: PAGE.l - 2 * MARGE, h: 330 };
  const y3d = PAGE.h - MARGE - 52 - zone3d.h;
  let image3d: PDFImage | null = null;
  const v3 = new Vue3D(doc);
  try {
    v3.fondImage = [1, 1, 1];
    await v3.charger();
    image3d = await pdf.embedPng(await v3.capture(Math.round(zone3d.w * 3), Math.round(zone3d.h * 3), "dessus"));
  } catch {
    image3d = null;
  } finally {
    v3.detruire();
  }
  if (image3d) garde.drawImage(image3d, { x: zone3d.x, y: y3d, width: zone3d.w, height: zone3d.h });
  else garde.drawRectangle({ x: zone3d.x, y: y3d, width: zone3d.w, height: zone3d.h, color: CLAIR });
  let y = y3d - 26;
  ecrire(garde, L("Caractéristiques", "Specifications"), MARGE, y, 12, gras);
  y -= 8;
  garde.drawLine({ start: { x: MARGE, y }, end: { x: PAGE.l - MARGE, y }, thickness: 0.6, color: ORANGE });
  y -= 16;
  for (const [k, v] of caracteristiques) {
    if (v === null || v === undefined || v === "") continue;
    if (y < MARGE + 10) break;
    ecrire(garde, k, MARGE, y, 9.5, gras, GRIS);
    ecrire(garde, couper(String(typeof v === "number" ? nombre(v, 0) : tm(v)), PAGE.l - 2 * MARGE - 170, 9.5, police), MARGE + 170, y, 9.5);
    y -= 15;
  }

  // --- Vues de dessus et de dessous ----------------------------------------------------------------------------
  if (b) {
    const page = nouvellePage(L("Vues de la carte", "Board views"));
    const m = Math.max(b[2] - b[0], b[3] - b[1]) * 0.03;
    const cadre: [number, number, number, number] = [b[0] - m, b[1] - m, b[2] + m, b[3] + m];
    const dispo = { w: PAGE.l - 2 * MARGE, h: (PAGE.h - 2 * MARGE - 40 - 2 * 22 - 16) / 2 };
    const e = Math.min(dispo.w / (cadre[2] - cadre[0]), dispo.h / (cadre[3] - cadre[1]));
    const wPt = (cadre[2] - cadre[0]) * e;
    const hPt = (cadre[3] - cadre[1]) * e;
    let yv = PAGE.h - MARGE - 34;
    for (const [cote, titre] of [["dessus", L("Dessus", "Top")], ["dessous", L("Dessous (vue en miroir, comme la carte retournée)", "Bottom (mirrored, as if the board were flipped)")]] as const) {
      ecrire(page, titre, MARGE, yv - 10, 10, gras);
      yv -= 18;
      try {
        const png = await vue2d(o, cote, cadre, Math.round(wPt * 3), Math.round(hPt * 3));
        page.drawImage(await pdf.embedPng(png), { x: MARGE + (dispo.w - wPt) / 2, y: yv - hPt, width: wPt, height: hPt });
      } catch {
        page.drawRectangle({ x: MARGE, y: yv - hPt, width: wPt, height: hPt, color: CLAIR });
      }
      yv -= hPt + 20;
    }
    ecrire(page, L(`Cotes extérieures : ${longueur(b[2] - b[0], unite, 2)} × ${longueur(b[3] - b[1], unite, 2)}`, `Overall size: ${longueur(b[2] - b[0], unite, 2)} × ${longueur(b[3] - b[1], unite, 2)}`), MARGE, MARGE - 6, 8, police, GRIS);
  }

  // --- Tableaux --------------------------------------------------------------------------------------------------
  let courante: PDFPage | null = null;
  let yt = 0;
  const TAILLE = 7.5;
  const LIGNE = 11.5;
  /** Tableau sur une ou plusieurs pages ; `largeurs` en fractions de la largeur utile. */
  const tableau = (titre: string, entetes: string[], largeurs: number[], lignes: (string | number | null | undefined)[][], droite: boolean[] = []) => {
    if (!lignes.length) return;
    const utile = PAGE.l - 2 * MARGE;
    const xs = largeurs.reduce<number[]>((acc, l, i) => [...acc, (acc[i] ?? MARGE) + l * utile], [MARGE]);
    const enTete = () => {
      courante!.drawRectangle({ x: MARGE, y: yt - 3.5, width: utile, height: LIGNE + 1, color: CLAIR });
      entetes.forEach((e, i) => {
        const txt = couper(e, xs[i + 1] - xs[i] - 6, TAILLE, gras);
        ecrire(courante!, txt, droite[i] ? xs[i + 1] - 4 - gras.widthOfTextAtSize(txt, TAILLE) : xs[i] + 2, yt, TAILLE, gras, GRIS);
      });
      yt -= LIGNE + 2;
    };
    const titrer = (suite: boolean) => {
      ecrire(courante!, suite ? L(`${titre} (suite)`, `${titre} (continued)`) : titre, MARGE, yt, 11, gras);
      yt -= 8;
      courante!.drawLine({ start: { x: MARGE, y: yt }, end: { x: PAGE.l - MARGE, y: yt }, thickness: 0.6, color: ORANGE });
      yt -= 14;
      enTete();
    };
    if (!courante || yt < MARGE + 80) {
      courante = nouvellePage(L("Données de la carte", "Board data"));
      yt = PAGE.h - MARGE - 36;
    } else yt -= 12;
    titrer(false);
    for (const [n, l] of lignes.entries()) {
      if (yt < MARGE + 6) {
        courante = nouvellePage(L("Données de la carte", "Board data"));
        yt = PAGE.h - MARGE - 36;
        titrer(true);
      }
      if (n % 2 === 1) courante!.drawRectangle({ x: MARGE, y: yt - 3, width: utile, height: LIGNE, color: rgb(0.975, 0.978, 0.982) });
      l.forEach((v, i) => {
        const s = v === null || v === undefined ? "" : typeof v === "number" ? nombre(v, 3) : String(v);
        const txt = couper(s, xs[i + 1] - xs[i] - 6, TAILLE, police);
        ecrire(courante!, txt, droite[i] ? xs[i + 1] - 4 - police.widthOfTextAtSize(txt, TAILLE) : xs[i] + 2, yt, TAILLE);
      });
      yt -= LIGNE;
    }
    yt -= 6;
  };

  if (meta.job?.empilage.length) {
    tableau(L("Empilage (fichier de job)", "Stackup (job file)"), [L("Couche", "Layer"), L("Type", "Type"), L("Épaisseur (mm)", "Thickness (mm)"), L("Matériau", "Material")], [0.32, 0.25, 0.18, 0.25],
      meta.job.empilage.map((l) => [l.nom, l.type, l.epaisseur ?? "", l.materiau]), [false, false, true, false]);
  }
  tableau(L("Couches", "Layers"), [L("Couche", "Layer"), L("Fichier", "File"), "Format", L("Objets", "Objects")], [0.34, 0.38, 0.14, 0.14],
    d.couches.filter((c) => c.objets > 0).sort((a, c) => c.role.rang - a.role.rang).map((c) => [tm(c.nom), c.fichier.split("/").pop(), c.meta.format ?? c.format, c.objets]), [false, false, false, true]);
  if (outils.length) {
    tableau(L("Perçages", "Drills"), [L("Couche", "Layer"), L("Outil", "Tool"), L("Type", "Type"), L("Diamètre", "Diameter"), L("Trous", "Holes")], [0.36, 0.1, 0.24, 0.16, 0.14],
      outils.map((t) => [t.couche, t.outil, t.genre, t.diametre >= 0.001 ? longueur(t.diametre, unite, 3) : L("non indiqué", "not stated"), t.trous]), [false, false, false, true, true]);
  }
  if (composants.length) {
    const tri = [...composants].sort((a, c) => a.reference.localeCompare(c.reference, undefined, { numeric: true }));
    tableau(L("Composants", "Components"), [L("Repère", "Ref."), L("Valeur", "Value"), L("Boîtier", "Package"), L("Côté", "Side"), "X", "Y", L("Rot.", "Rot.")], [0.11, 0.2, 0.33, 0.1, 0.09, 0.09, 0.08],
      tri.map((c) => [c.reference, c.valeur, c.boitier, tm(c.cote), typeof c.x === "number" ? longueur(c.x, unite, 2) : "", typeof c.y === "number" ? longueur(c.y, unite, 2) : "", typeof c.rotation === "number" ? `${nombre(c.rotation, 1)}°` : c.rotation ? `${c.rotation}°` : ""]),
      [false, false, false, false, true, true, true]);
  }
  if (meta.nomenclature?.length) {
    tableau(L("Nomenclature", "Bill of materials"), [L("Qté", "Qty"), L("Pièce", "Part"), L("Valeur", "Value"), L("Repères", "References")], [0.07, 0.33, 0.2, 0.4],
      meta.nomenclature.map((l) => [l.quantite, l.piece, l.valeur, l.references.join(", ")]), [true, false, false, false]);
  }
  if (meta.nets?.length) {
    // Quatre nets par ligne : la liste reste compacte.
    const noms = [...meta.nets].map((n) => n.nom).sort((a, c) => a.localeCompare(c, undefined, { numeric: true }));
    const lignes: string[][] = [];
    for (let i = 0; i < noms.length; i += 4) lignes.push(noms.slice(i, i + 4));
    tableau(L(`Nets (${noms.length})`, `Nets (${noms.length})`), ["", "", "", ""], [0.25, 0.25, 0.25, 0.25], lignes);
  }
  const alertes = [...d.alertes, ...(meta.odb?.alertes ?? []), ...d.couches.flatMap((c) => (c.meta.alertes ?? []).map((a) => `${tm(c.nom)} : ${a}`))];
  if (alertes.length) tableau(L("Remarques de lecture", "Reading notes"), [L("Remarque", "Note")], [1], alertes.map((a) => [tm(a)]));

  // Pieds de page : numéro et total.
  pages.forEach((p, i) => ecrire(p, `${i + 1} / ${numero}`, PAGE.l - MARGE - 24, 20, 8, police, GRIS));
  const octets = await pdf.save();
  telecharger(octets as BlobPart, `${L("fiche", "datasheet")} ${doc.nom}.pdf`.replace(/[\\/:*?"<>|]/g, "_"), "application/pdf");
}
