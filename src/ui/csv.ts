/**
 * Export CSV des tableaux, lisible tel quel par Excel en français : séparateur « ; », BOM UTF-8 en tête, champs entre
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

export type CelluleCsv = string | number | null | undefined;

export interface TableCsv {
  entetes: string[];
  lignes: CelluleCsv[][];
}

/** Nombre à la française, sans séparateur de milliers (Excel le lit alors comme un nombre). */
export function nombreCsv(v: number, decimales = 6): string {
  if (!Number.isFinite(v)) return "";
  return v.toLocaleString("fr-FR", { minimumFractionDigits: 0, maximumFractionDigits: decimales, useGrouping: false });
}

function cellule(v: CelluleCsv): string {
  if (v === null || v === undefined) return "";
  const s = typeof v === "number" ? nombreCsv(v) : v;
  return /[;"\r\n]/.test(s) || /^\s|\s$/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

/** Texte CSV complet, BOM UTF-8 compris. */
export function versCsv(t: TableCsv): string {
  const lignes = [t.entetes, ...t.lignes].map((l) => l.map(cellule).join(";"));
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
    title: "Exporter ce tableau en CSV",
    "aria-label": `Exporter le tableau « ${tableau} » en CSV`,
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
    entetes: ["Couche", "Fichier", "Format", "Rôle", "Objets", "Remarques"],
    lignes: couches.map((c) => [c.desc.nom, c.desc.fichier, c.desc.meta.format ?? c.desc.format, c.desc.role.libelle, c.desc.objets, (c.desc.meta.alertes ?? []).join(" ; ")]),
  };
}

const ETATS: Record<EtatPaire, string> = {
  "a-analyser": "à analyser",
  analyse: "analyse en cours",
  identique: "identique",
  differences: "différences",
  "seulement-a": "seulement dans A",
  "seulement-b": "seulement dans B",
  representation: "représentations différentes",
};

/** Couches comparées (panneau de gauche en comparaison). */
export function csvPaires(paires: Paire[]): TableCsv {
  return {
    entetes: ["Couche", "Fichier A", "Format A", "Fichier B", "Format B", "État", "Zones de différence"],
    lignes: paires.map((p) => [p.libelle, p.a?.desc.fichier, p.a?.desc.format, p.b?.desc.fichier, p.b?.desc.format, ETATS[p.etat], p.etat === "differences" ? p.zones.length : null]),
  };
}

/** Zones de différence d'une couche, dans l'unité d'affichage (surfaces en mm²). */
export function csvZones(p: Paire, unite: Unite): TableCsv {
  const u = libelleUnite(unite);
  const v = (mm: number) => Number(enUnite(mm, unite).toFixed(unite === "pouce" ? 5 : unite === "mil" ? 2 : 4));
  return {
    entetes: ["N°", "Nature", `X centre (${u})`, `Y centre (${u})`, `Largeur (${u})`, `Hauteur (${u})`, `X min (${u})`, `Y min (${u})`, `X max (${u})`, `Y max (${u})`, "Surface disparue (mm²)", "Surface apparue (mm²)"],
    lignes: p.zones.map((z, i) => [
      i + 1,
      z.disparu > 0 && z.apparu > 0 ? "modifié" : z.disparu > 0 ? "disparu" : "apparu",
      v((z.x0 + z.x1) / 2), v((z.y0 + z.y1) / 2), v(z.x1 - z.x0), v(z.y1 - z.y0), v(z.x0), v(z.y0), v(z.x1), v(z.y1),
      Number(z.disparu.toFixed(4)), Number(z.apparu.toFixed(4)),
    ]),
  };
}

/** Composants retirés, ajoutés et modifiés entre A et B. */
export function csvDifferencesComposants(d: DifferencesComposants, b: Composant[]): TableCsv {
  const ligne = (genre: string, c: Composant | undefined, ref: string, detail: string): CelluleCsv[] => [genre, ref, c?.valeur, c?.boitier, c?.piece, c?.cote, detail];
  return {
    entetes: ["Changement", "Repère", "Valeur", "Boîtier", "Pièce", "Côté", "Détail"],
    lignes: [
      ...d.retires.map((c) => ligne("retiré", c, c.reference, "")),
      ...d.ajoutes.map((c) => ligne("ajouté", c, c.reference, "")),
      ...d.modifies.map((m) => ligne("modifié", b.find((x) => x.reference === m.reference), m.reference, m.changements.join(" ; "))),
    ],
  };
}
