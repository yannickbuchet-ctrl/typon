/**
 * Un jeu de fichiers ouvert (Gerber, perçages, ODB++…) : sa description par le moteur et l'état d'affichage de chaque
 * couche (visibilité, couleur, opacité, géométrie envoyée à la carte graphique).
 */
import { moteur, type Description, type DescriptionCouche, type Genre } from "../moteur/client";
import type { CoucheGpu, Rendu } from "../rendu/rendu";

export interface EtatCouche {
  desc: DescriptionCouche;
  visible: boolean;
  couleur: string;
  opacite: number;
  gpu?: CoucheGpu;
  chargement?: Promise<CoucheGpu>;
}

export type Cote = "dessus" | "dessous";

/** Opacité par défaut, selon le genre de couche. */
const OPACITE: Partial<Record<Genre, number>> = {
  cuivre: 0.85,
  masque: 0.45,
  serigraphie: 0.95,
  pate: 0.65,
  percage: 1,
  fraisage: 1,
  contour: 1,
  composants: 0.85,
};

export class DocumentTypon {
  couches: EtatCouche[];
  constructor(
    readonly nom: string,
    readonly lot: number,
    readonly description: Description,
    readonly taille: number,
    readonly duree: number,
  ) {
    this.couches = description.couches.map((desc) => ({ desc, visible: false, couleur: desc.role.couleur, opacite: OPACITE[desc.role.genre] ?? 0.7 }));
    this.preregler("dessus");
  }

  /** Visibilité par défaut pour un côté : couches de ce côté, perçages et contour ; le reste masqué. */
  preregler(cote: Cote | "cuivre" | "tout") {
    for (const c of this.couches) {
      const r = c.desc.role;
      if (cote === "tout") {
        c.visible = c.desc.objets > 0 && r.genre !== "autre";
        continue;
      }
      if (cote === "cuivre") {
        c.visible = r.genre === "cuivre" || r.genre === "contour" || r.genre === "percage";
        continue;
      }
      const commun = r.genre === "percage" || r.genre === "fraisage" || r.genre === "contour";
      const memeCote = r.cote === cote || (r.cote === "aucun" && r.genre !== "autre" && r.genre !== "plan" && r.genre !== "mecanique");
      c.visible = c.desc.objets > 0 && (commun || (memeCote && ["cuivre", "masque", "serigraphie", "composants"].includes(r.genre)));
    }
    // Rien de visible (par exemple un seul fichier « autre ») : on affiche tout.
    if (!this.couches.some((c) => c.visible)) for (const c of this.couches) c.visible = c.desc.objets > 0 || this.couches.length === 1;
  }

  /** Géométrie de la couche, chargée à la demande depuis le moteur. */
  gpu(rendu: Rendu, c: EtatCouche): Promise<CoucheGpu> {
    if (c.gpu) return Promise.resolve(c.gpu);
    c.chargement ??= moteur.geometrie(this.lot, c.desc.id).then((g) => {
      c.gpu = rendu.charger(g);
      return c.gpu;
    });
    return c.chargement;
  }

  /** Boîte de la carte : contour s'il y en a un, sinon toutes les couches. */
  get boite(): [number, number, number, number] | null {
    const b = this.description.meta.boite;
    return b && b[2] > b[0] ? b : null;
  }

  fermer(rendu: Rendu) {
    for (const c of this.couches) if (c.gpu) rendu.liberer(c.gpu);
    void moteur.fermer(this.lot);
  }
}

/** Lit des fichiers (déposés, choisis, ou un exemple) et en fait un document. */
export async function ouvrir(fichiers: File[], nom?: string): Promise<DocumentTypon> {
  const lus = await Promise.all(fichiers.map(async (f) => ({ nom: (f as File & { chemin?: string }).chemin ?? f.webkitRelativePath ?? f.name, octets: await f.arrayBuffer() })));
  for (const f of lus) if (!f.nom) f.nom = "fichier";
  const taille = lus.reduce((s, f) => s + f.octets.byteLength, 0);
  const r = await moteur.analyser(lus.map((f) => ({ nom: f.nom || "fichier", octets: f.octets })));
  return new DocumentTypon(nom ?? nomDuJeu(fichiers), r.lot, r.description, taille, r.duree);
}

function nomDuJeu(fichiers: File[]): string {
  if (fichiers.length === 1) return fichiers[0].name;
  const dossier = fichiers[0]?.webkitRelativePath?.split("/")[0];
  if (dossier) return dossier;
  // Préfixe commun des noms (souvent le nom du projet).
  const noms = fichiers.map((f) => f.name);
  let p = noms[0] ?? "";
  for (const n of noms) while (p && !n.startsWith(p)) p = p.slice(0, -1);
  p = p.replace(/[-_. ]+$/, "");
  return p.length >= 3 ? p : `${fichiers.length} fichiers`;
}
