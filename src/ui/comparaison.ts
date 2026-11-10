/**
 * Comparaison de deux jeux (A : version de référence, B : nouvelle version) : appariement des couches (même rôle,
 * même côté, même numéro, quel que soit le format), alignement des origines, recherche des zones de différence.
 */
import { langue, t } from "../i18n";
import { traduireMoteur } from "../i18n-moteur";
import { moteur, type Composant } from "../moteur/client";
import type { CoucheAffichee, Rendu, Vue } from "../rendu/rendu";
import type { DocumentTypon, EtatCouche } from "./document";

export interface ZoneMm {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  /** Surface (mm²) seulement dans A (disparue) et seulement dans B (apparue). */
  disparu: number;
  apparu: number;
}

export type EtatPaire = "a-analyser" | "analyse" | "identique" | "differences" | "seulement-a" | "seulement-b" | "representation";

export interface Paire {
  cle: string;
  libelle: string;
  a?: EtatCouche;
  b?: EtatCouche;
  etat: EtatPaire;
  zones: ZoneMm[];
  resolution?: number;
}

/** Clé d'appariement d'une couche : genre, côté, numéro (cuivre interne), sinon libellé. */
function cle(c: EtatCouche): string {
  const r = c.desc.role;
  if (r.genre === "cuivre") return `cuivre:${r.cote}:${r.cote === "interne" ? r.numero : ""}`;
  if (["masque", "serigraphie", "pate", "composants"].includes(r.genre)) return `${r.genre}:${r.cote}`;
  if (r.genre === "contour") return "contour";
  if (r.genre === "percage" || r.genre === "fraisage") {
    // « Perçage métallisé 1–4 », « Perçage « d_1_2 » »… : on garde le type (métallisé ou non) s'il est connu.
    const l = c.desc.role.libelle.toLowerCase();
    return `percage:${l.includes("non") ? "npth" : l.includes("métallisé") ? "pth" : l}`;
  }
  return `autre:${c.desc.nom.toLowerCase()}`;
}

export function apparier(a: DocumentTypon, b: DocumentTypon): Paire[] {
  const paires: Paire[] = [];
  const restantsB = [...b.couches.filter((c) => c.desc.objets > 0)];
  for (const ca of a.couches.filter((c) => c.desc.objets > 0)) {
    const k = cle(ca);
    let i = restantsB.findIndex((cb) => cle(cb) === k);
    if (i < 0) i = restantsB.findIndex((cb) => cb.desc.fichier.toLowerCase() === ca.desc.fichier.toLowerCase());
    if (i < 0 && k.startsWith("percage")) i = restantsB.findIndex((cb) => cle(cb).startsWith("percage"));
    const cb = i >= 0 ? restantsB.splice(i, 1)[0] : undefined;
    // Couches de composants de formats différents (points du Gerber X3, contours des boîtiers en ODB++ ou IPC-2581) : leur dessin
    // n'est pas comparable ; la liste des composants l'est.
    const representation = cb && ca.desc.role.genre === "composants" && ca.desc.format !== cb.desc.format;
    paires.push({ cle: k, libelle: ca.desc.nom, a: ca, b: cb, etat: !cb ? "seulement-a" : representation ? "representation" : "a-analyser", zones: [] });
  }
  for (const cb of restantsB) paires.push({ cle: cle(cb), libelle: cb.desc.nom, b: cb, etat: "seulement-b", zones: [] });
  paires.sort((p, q) => ((q.a ?? q.b)!.desc.role.rang - (p.a ?? p.b)!.desc.role.rang));
  return paires;
}

/**
 * Décalage à appliquer à B pour l'aligner sur A : les deux jeux n'ont pas toujours la même origine (un export Gerber
 * et un export ODB++, ou une carte déplacée dans le logiciel de conception). On aligne les coins inférieurs gauches des
 * cartes quand leurs dimensions sont les mêmes (à 0,5 mm près) mais qu'elles sont placées différemment.
 */
export function alignementAuto(a: DocumentTypon, b: DocumentTypon): { dx: number; dy: number } {
  const ba = a.boite;
  const bb = b.boite;
  if (!ba || !bb) return { dx: 0, dy: 0 };
  const memeTaille = Math.abs(ba[2] - ba[0] - (bb[2] - bb[0])) < 0.5 && Math.abs(ba[3] - ba[1] - (bb[3] - bb[1])) < 0.5;
  const dx = ba[0] - bb[0];
  const dy = ba[1] - bb[1];
  if (!memeTaille || Math.hypot(dx, dy) < 0.001) return { dx: 0, dy: 0 };
  return { dx, dy };
}

/**
 * Cherche les différences d'une paire : rendu des deux couches sur une même grille (jusqu'à 4096 pixels de côté),
 * écarts plus fins que `tolerance` (mm) ignorés, puis regroupement en zones par le moteur.
 */
export async function analyser(rendu: Rendu, docA: DocumentTypon, docB: DocumentTypon, p: Paire, decalage: { dx: number; dy: number }, tolerance: number): Promise<void> {
  if (!p.a || !p.b) return;
  p.etat = "analyse";
  const [ga, gb] = await Promise.all([docA.gpu(rendu, p.a), docB.gpu(rendu, p.b)]);
  const boite = [Math.min(ga.boite[0], gb.boite[0] + decalage.dx), Math.min(ga.boite[1], gb.boite[1] + decalage.dy), Math.max(ga.boite[2], gb.boite[2] + decalage.dx), Math.max(ga.boite[3], gb.boite[3] + decalage.dy)];
  const l = boite[2] - boite[0];
  const h = boite[3] - boite[1];
  if (!(l > 0 && h > 0)) {
    p.etat = "identique";
    return;
  }
  const marge = Math.max(l, h) * 0.01 + 0.2;
  const L = l + 2 * marge;
  const H = h + 2 * marge;
  // Résolution : 10 µm par pixel au mieux, 4096 pixels de côté au plus.
  const max = Math.min(4096, rendu.gl.getParameter(rendu.gl.MAX_RENDERBUFFER_SIZE) as number);
  const px = Math.max(0.01, L / max, H / max);
  const largeur = Math.ceil(L / px);
  const hauteur = Math.ceil(H / px);
  const vue: Vue = { cx: boite[0] - marge + (largeur * px) / 2, cy: boite[1] - marge + (hauteur * px) / 2, echelle: 1 / px, miroir: false };
  const ca: CoucheAffichee = { gpu: ga, couleur: [1, 1, 1], opacite: 1 };
  const cb: CoucheAffichee = { gpu: gb, couleur: [1, 1, 1], opacite: 1, dx: decalage.dx, dy: decalage.dy };
  const masque = rendu.masques([ca], [cb], vue, largeur, hauteur);
  // Différences : bit 0 = seulement A, bit 1 = seulement B.
  const diff = new Uint8Array(largeur * hauteur);
  for (let i = 0; i < diff.length; i++) {
    const v = masque[i];
    diff[i] = v === 1 ? 1 : v === 2 ? 2 : 0;
  }
  // Tolérance : érosion des différences (un écart de moins de `tolerance` mm de large disparaît).
  const passes = Math.min(20, Math.round(tolerance / px / 2));
  eroder(diff, largeur, hauteur, passes);
  const fusion = Math.max(2, Math.round(0.5 / px));
  const zones = await moteur.zones(diff, largeur, hauteur, fusion, 2);
  const x0 = boite[0] - marge;
  const y1 = boite[1] - marge + hauteur * px;
  // Les zones ont été érodées : on les élargit d'autant pour qu'elles couvrent la différence entière.
  const e = passes * px + px;
  p.zones = zones.map((z) => ({
    x0: x0 + z.x0 * px - e,
    x1: x0 + z.x1 * px + e,
    y0: y1 - z.y1 * px - e,
    y1: y1 - z.y0 * px + e,
    disparu: z.a * px * px,
    apparu: z.b * px * px,
  }));
  p.resolution = px;
  p.etat = p.zones.length ? "differences" : "identique";
}

function eroder(m: Uint8Array, l: number, h: number, passes: number) {
  if (passes <= 0) return;
  let src: Uint8Array = m;
  let dst: Uint8Array = new Uint8Array(m.length);
  for (let k = 0; k < passes; k++) {
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < l; x++) {
        const i = y * l + x;
        const v = src[i];
        if (!v || x === 0 || y === 0 || x === l - 1 || y === h - 1) {
          dst[i] = 0;
          continue;
        }
        dst[i] = src[i - 1] && src[i + 1] && src[i - l] && src[i + l] ? v : 0;
      }
    }
    [src, dst] = [dst, src];
  }
  if (src !== m) m.set(src);
}

export interface DifferencesComposants {
  ajoutes: Composant[];
  retires: Composant[];
  modifies: { reference: string; changements: string[] }[];
  communs: number;
}

/** Compare les listes de composants (repère, valeur, boîtier, pièce, position, rotation, côté). */
export function comparerComposants(a: Composant[], b: Composant[], decalage: { dx: number; dy: number }): DifferencesComposants {
  const parRef = (l: Composant[]) => new Map(l.map((c) => [c.reference, c]));
  const ma = parRef(a);
  const mb = parRef(b);
  const r: DifferencesComposants = { ajoutes: [], retires: [], modifies: [], communs: 0 };
  for (const [ref, ca] of ma) {
    const cb = mb.get(ref);
    if (!cb) {
      r.retires.push(ca);
      continue;
    }
    r.communs++;
    const changements: string[] = [];
    const comparer = (nom: string, va: unknown, vb: unknown) => {
      const sa = String(va ?? "").trim();
      const sb = String(vb ?? "").trim();
      if (sa && sb && sa !== sb) changements.push(t("cmp.changement", { nom, a: sa, b: sb }));
    };
    comparer(t("cmp.changement.valeur"), ca.valeur, cb.valeur);
    comparer(t("cmp.changement.boitier"), ca.boitier, cb.boitier);
    comparer(t("cmp.changement.piece"), ca.piece, cb.piece);
    const rot = (v: number | string) => {
      const n = Number(String(v).replace(",", "."));
      return Number.isFinite(n) ? ((n % 360) + 360) % 360 : NaN;
    };
    const ra = rot(ca.rotation);
    const rb = rot(cb.rotation);
    if (Number.isFinite(ra) && Number.isFinite(rb) && Math.abs(ra - rb) > 0.05) changements.push(t("cmp.changement", { nom: t("cmp.changement.rotation"), a: `${ra}°`, b: `${rb}°` }));
    if (ca.x !== undefined && cb.x !== undefined && ca.y !== undefined && cb.y !== undefined) {
      const d = Math.hypot(cb.x + decalage.dx - ca.x, cb.y + decalage.dy - ca.y);
      if (d > 0.01) changements.push(t("cmp.changement.deplace", { d: langue() === "fr" ? d.toFixed(3).replace(".", ",") : d.toFixed(3) }));
    }
    if (ca.cote && cb.cote && ca.cote !== cb.cote) changements.push(t("cmp.changement", { nom: t("cmp.changement.cote"), a: traduireMoteur(ca.cote), b: traduireMoteur(cb.cote) }));
    if (changements.length) r.modifies.push({ reference: ref, changements });
  }
  for (const [ref, cb] of mb) if (!ma.has(ref)) r.ajoutes.push(cb);
  const tri = (x: { reference: string }, y: { reference: string }) => x.reference.localeCompare(y.reference, "fr", { numeric: true });
  r.ajoutes.sort(tri);
  r.retires.sort(tri);
  r.modifies.sort(tri);
  return r;
}
