/**
 * Export CSV des tableaux, lisible tel quel par Excel : séparateur « ; » en français (« , » en anglais), BOM UTF-8 en tête, champs entre
 * guillemets quand il le faut (guillemets doublés), fins de ligne CRLF, nombres à virgule décimale.
 *
 * Le CSV est toujours construit à partir des données sources du tableau, jamais du DOM : il contient donc toutes les
 * lignes, y compris celles que l'affichage tronque ou que le filtre masque.
 */
import type { Composant } from "../moteur/client";
import type { DifferencesComposants, EtatPaire, Paire } from "./comparaison";
import type { EtatCouche } from "./document";
import { el, telecharger, type Unite } from "./dom";
import { icone } from "./icones";
import { langue } from "../i18n";
import { traduireMoteur as tm } from "../i18n-moteur";

/** Texte selon la langue de l'interface (en-têtes et libellés propres aux exports). */
const L = (fr: string, en: string) => (langue() === "en" ? en : fr);

export type CelluleCsv = string | number | null | undefined;

export interface TableCsv {
  entetes: string[];
  lignes: CelluleCsv[][];
}

/** Nombre à la française (ou à l'anglaise en anglais), sans séparateur de milliers (Excel le lit alors comme un
 * nombre). */
export function nombreCsv(v: number, decimales = 6): string {
  if (!Number.isFinite(v)) return "";
  return v.toLocaleString(langue() === "en" ? "en-US" : "fr-FR", { minimumFractionDigits: 0, maximumFractionDigits: decimales, useGrouping: false });
}

function cellule(v: CelluleCsv): string {
  if (v === null || v === undefined) return "";
  let s = typeof v === "number" ? nombreCsv(v) : v;
  // Texte qui commence par = + - @ (nets « +3V3 », « -12V », ou contenu piégé « =HYPERLINK(…) ») : Excel le
  // prendrait pour une formule. Il est écrit en formule constante ="…", qui affiche le texte tel quel.
  if (typeof v === "string" && /^[=+\-@\t\r]/.test(s)) s = `="${s.replace(/"/g, '""')}"`;
  return /[;,"\r\n]/.test(s) || /^\s|\s$/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

/** Texte CSV complet, BOM UTF-8 compris. */
export function versCsv(t: TableCsv): string {
  // Excel en français attend « ; » ; en anglais, la virgule.
  const separateur = langue() === "en" ? "," : ";";
  const lignes = [t.entetes, ...t.lignes].map((l) => l.map(cellule).join(separateur));
  return `﻿${lignes.join("\r\n")}\r\n`;
}

/** Nom de fichier « <jeu>-<tableau>.csv », sans les caractères interdits par Windows. */
export function nomFichierCsv(jeu: string, tableau: string): string {
  const propre = (s: string) => s.replace(/[\\/:*?"<>|\u0000-\u001f]/g, "_").replace(/\s+/g, " ").trim().replace(/^\.+|\.+$/g, "");
  const nom = [propre(jeu), propre(tableau)].filter(Boolean).join("-") || "tableau";
  return `${nom.slice(0, 180)}.csv`;
}

export function telechargerCsv(jeu: string, tableau: string, t: TableCsv) {
  telecharger(versCsv(t), nomFichierCsv(jeu, tableau), "text/csv;charset=utf-8");
}

/** Petit bouton discret « CSV » qui exporte le tableau produit à la demande. */
export function boutonCsv(jeu: string, tableau: string, produire: () => TableCsv): HTMLButtonElement {
  return el("button", {
    type: "button",
    class: "bouton-csv",
    title: L("Exporter ce tableau en CSV", "Export this table as CSV"),
    "aria-label": L(`Exporter le tableau « ${tableau} » en CSV`, `Export the “${tableau}” table as CSV`),
    "data-tableau": tableau,
    html: `${icone("telecharger", 13)}<span>CSV</span>`,
    onclick: (e: Event) => {
      e.stopPropagation();
      telechargerCsv(jeu, tableau, produire());
    },
  });
}

/**
 * Place le bouton CSV d'un tableau à côté de son titre : le bouton créé par `tableau()` dans sa barre (ou un bouton
 * passé tel quel) est déplacé dans le titre (h3, h4…).
 */
export function boutonVersTitre(titre: HTMLElement, ...contenu: unknown[]): HTMLElement {
  for (const c of contenu) {
    if (!(c instanceof HTMLElement)) continue;
    const b = c.classList.contains("bouton-csv") ? c : c.querySelector(":scope > .barre-tableau > .bouton-csv");
    if (b) {
      titre.classList.add("titre-avec-csv");
      titre.append(b);
      break;
    }
  }
  return titre;
}

/** Longueur (mm) convertie dans l'unité d'affichage, et le libellé d'unité pour l'en-tête de colonne. */
export function enUnite(mm: number, unite: Unite): number {
  return unite === "mil" ? mm / 0.0254 : unite === "pouce" ? mm / 25.4 : mm;
}

export function libelleUnite(unite: Unite): string {
  return unite === "mil" ? "mil" : unite === "pouce" ? "po" : "mm";
}

/* --- Tableaux construits en dehors de `tableau()` : fichiers, comparaison ------------------------------------------- */

/** Couches lues (onglet Fichiers). */
export function csvFichiers(couches: EtatCouche[]): TableCsv {
  return {
    entetes: [L("Couche", "Layer"), L("Fichier", "File"), "Format", L("Rôle", "Role"), L("Objets", "Objects"), L("Remarques", "Notes")],
    lignes: couches.map((c) => [tm(c.desc.nom), c.desc.fichier, c.desc.meta.format ?? c.desc.format, tm(c.desc.role.libelle), c.desc.objets, (c.desc.meta.alertes ?? []).map(tm).join(" ; ")]),
  };
}

const ETATS: Record<EtatPaire, [string, string]> = {
  "a-analyser": ["à analyser", "not analysed"],
  analyse: ["analyse en cours", "analysing"],
  identique: ["identique", "identical"],
  differences: ["différences", "differences"],
  "seulement-a": ["seulement dans A", "only in A"],
  "seulement-b": ["seulement dans B", "only in B"],
  representation: ["représentations différentes", "different representations"],
};

/** Couches comparées (panneau de gauche en comparaison). */
export function csvPaires(paires: Paire[]): TableCsv {
  return {
    entetes: [L("Couche", "Layer"), L("Fichier A", "File A"), "Format A", L("Fichier B", "File B"), "Format B", L("État", "Status"), L("Zones de différence", "Difference areas")],
    lignes: paires.map((p) => [tm(p.libelle), p.a?.desc.fichier, p.a?.desc.format, p.b?.desc.fichier, p.b?.desc.format, L(...ETATS[p.etat]), p.etat === "differences" ? p.zones.length : null]),
  };
}

/** Zones de différence d'une couche, dans l'unité d'affichage (surfaces en mm²). */
export function csvZones(p: Paire, unite: Unite): TableCsv {
  const u = libelleUnite(unite);
  const v = (mm: number) => Number(enUnite(mm, unite).toFixed(unite === "pouce" ? 5 : unite === "mil" ? 2 : 4));
  return {
    entetes: [L("N°", "No."), L("Nature", "Kind"), L(`X centre (${u})`, `X centre (${u})`), L(`Y centre (${u})`, `Y centre (${u})`), L(`Largeur (${u})`, `Width (${u})`), L(`Hauteur (${u})`, `Height (${u})`), `X min (${u})`, `Y min (${u})`, `X max (${u})`, `Y max (${u})`, L("Surface disparue (mm²)", "Removed area (mm²)"), L("Surface apparue (mm²)", "Added area (mm²)")],
    lignes: p.zones.map((z, i) => [
      i + 1,
      z.disparu > 0 && z.apparu > 0 ? L("modifié", "changed") : z.disparu > 0 ? L("disparu", "removed") : L("apparu", "added"),
      v((z.x0 + z.x1) / 2), v((z.y0 + z.y1) / 2), v(z.x1 - z.x0), v(z.y1 - z.y0), v(z.x0), v(z.y0), v(z.x1), v(z.y1),
      Number(z.disparu.toFixed(4)), Number(z.apparu.toFixed(4)),
    ]),
  };
}

/** Composants retirés, ajoutés et modifiés entre A et B. */
export function csvDifferencesComposants(d: DifferencesComposants, b: Composant[]): TableCsv {
  const ligne = (genre: string, c: Composant | undefined, ref: string, detail: string): CelluleCsv[] => [genre, ref, c?.valeur, c?.boitier, c?.piece, c?.cote, detail];
  return {
    entetes: [L("Changement", "Change"), L("Repère", "Reference"), L("Valeur", "Value"), L("Boîtier", "Package"), L("Pièce", "Part"), L("Côté", "Side"), L("Détail", "Detail")],
    lignes: [
      ...d.retires.map((c) => ligne(L("retiré", "removed"), c, c.reference, "")),
      ...d.ajoutes.map((c) => ligne(L("ajouté", "added"), c, c.reference, "")),
      ...d.modifies.map((m) => ligne(L("modifié", "changed"), b.find((x) => x.reference === m.reference), m.reference, m.changements.join(" ; "))),
    ],
  };
}
