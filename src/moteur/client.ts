/**
 * Accès au moteur (Web Worker + WebAssembly) depuis la page, et types des données qu'il renvoie.
 */

export type Genre = "cuivre" | "masque" | "serigraphie" | "pate" | "contour" | "percage" | "fraisage" | "composants" | "mecanique" | "plan" | "netlist" | "job" | "autre";
export type Cote = "dessus" | "dessous" | "interne" | "aucun";

export interface Role {
  genre: Genre;
  cote: Cote;
  numero: number | null;
  libelle: string;
  couleur: string;
  rang: number;
  source: string;
}

export interface DescriptionCouche {
  id: number;
  nom: string;
  fichier: string;
  format: "Gerber" | "Excellon" | "ODB++" | "IPC-2581";
  role: Role;
  boite: [number, number, number, number];
  objets: number;
  meta: Record<string, unknown> & {
    format?: string;
    unite?: string;
    notation?: string;
    attributs?: [string, string][];
    commentaires?: string[];
    ouvertures?: { code: string; forme: string; fonction: string | null }[];
    outils?: { numero: number; diametre: number; trous: number; fraisages: number }[];
    alertes?: string[];
    nets?: number;
    composants?: number;
    trous?: number;
    typeOdb?: string;
    nomOdb?: string;
    elements?: number;
  };
}

export interface Composant {
  reference: string;
  valeur: string;
  boitier: string;
  piece: string;
  fabricant?: string;
  rotation: number | string;
  cote: string;
  x?: number;
  y?: number;
  broches?: number;
  /** Encombrement du boîtier dans son repère (x0, y0, x1, y1, mm). */
  corps?: [number, number, number, number] | null;
  /** Hauteur (mm), si le fichier la donne. */
  hauteur?: number | null;
  proprietes: [string, string][];
}

export interface Description {
  couches: DescriptionCouche[];
  meta: {
    boite: [number, number, number, number];
    couchesCuivre: number;
    epaisseur?: { valeur: number; source: string };
    finition?: string;
    job?: { fichier: string; rubriques: { titre: string; lignes: [string, string][] }[]; empilage: { nom: string; type: string; epaisseur: number | null; materiau: string }[] };
    odb?: {
      /** « ODB++ » ou « IPC-2581 » : les deux formats sont présentés de la même façon. */
      source?: string;
      info: [string, string][];
      etape: string;
      etapes: string[];
      empilage: { nom: string; type: string; contexte: string; role: string; debut: string; fin: string; negative: boolean; elements: number }[];
      outils: { couche: string; debut: string; fin: string; outils: { numero: string; genre: string; fini: number; percage: number; trous: number }[] }[];
      alertes: string[];
    };
    composants?: Composant[];
    nomenclature?: { piece: string; valeur: string; quantite: number; references: string[] }[];
    nets?: { nom: string; elements: number | null }[];
    netlist?: { fichier: string; nets: number };
    listesOuvertures?: { fichier: string; ouvertures: number; unite: string }[];
  };
  alertes: string[];
  ignores: string[];
}

/** Un lot de primitives de même polarité ; [début, longueur] dans le tableau de flottants de la couche. */
export interface LotGeometrie {
  sombre: boolean;
  caps: [number, number];
  boites: [number, number];
  tris: [number, number];
}

export interface Geometrie {
  lots: LotGeometrie[];
  boite: [number, number, number, number];
  donnees: Float32Array;
}

export interface ObjetTrouve {
  objet: number;
  info: [string, string][];
  boite: [number, number, number, number];
}

export interface Zone {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  a: number;
  b: number;
}

export interface Modele3d {
  anneaux: [number, number][][];
  /** « contour » (tracé de la carte) ou « rectangle » (pas de contour fermé : rectangle englobant). */
  source: "contour" | "rectangle";
  triangles: Float32Array;
}

class Moteur {
  private worker: Worker | null = null;
  private suivant = 1;
  private attente = new Map<number, { ok: (v: unknown) => void; ko: (e: Error) => void }>();

  private demarrer(): Worker {
    if (this.worker) return this.worker;
    const w = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
    w.onmessage = (e: MessageEvent<{ id: number; ok: boolean; resultat?: unknown; erreur?: string }>) => {
      const a = this.attente.get(e.data.id);
      if (!a) return;
      this.attente.delete(e.data.id);
      if (e.data.ok) a.ok(e.data.resultat);
      else a.ko(new Error(e.data.erreur));
    };
    w.onerror = (e) => {
      for (const a of this.attente.values()) a.ko(new Error(e.message || "Le moteur n'a pas pu démarrer."));
      this.attente.clear();
      this.worker = null;
    };
    this.worker = w;
    return w;
  }

  private appel<T>(message: Record<string, unknown>, transferts: Transferable[] = []): Promise<T> {
    const w = this.demarrer();
    const id = this.suivant++;
    return new Promise<T>((ok, ko) => {
      this.attente.set(id, { ok: ok as (v: unknown) => void, ko });
      w.postMessage({ id, ...message }, transferts);
    });
  }

  /** Lit des fichiers (archives comprises). Les ArrayBuffer sont transférés au moteur. */
  analyser(fichiers: { nom: string; octets: ArrayBuffer }[]): Promise<{ lot: number; description: Description; duree: number }> {
    return this.appel({ op: "analyser", fichiers }, fichiers.map((f) => f.octets));
  }

  async geometrie(lot: number, couche: number): Promise<Geometrie> {
    const r = await this.appel<{ geometrie: { lots: LotGeometrie[]; boite: [number, number, number, number] }; donnees: ArrayBuffer }>({ op: "geometrie", lot, couche });
    return { ...r.geometrie, donnees: new Float32Array(r.donnees) };
  }

  objet(lot: number, couche: number, x: number, y: number, tol: number): Promise<ObjetTrouve | null> {
    return this.appel({ op: "objet", lot, couche, x, y, tol });
  }

  zones(masque: Uint8Array, largeur: number, hauteur: number, fusion: number, minimum: number): Promise<Zone[]> {
    const copie = masque.slice().buffer;
    return this.appel({ op: "zones", masque: copie, largeur, hauteur, fusion, minimum }, [copie]);
  }

  /** Contour de la carte pour la vue 3D : anneaux (bord puis découpes) et triangles de la face (x, y). */
  async modele3d(lot: number): Promise<Modele3d> {
    const r = await this.appel<{ modele: Omit<Modele3d, "triangles">; donnees: ArrayBuffer }>({ op: "modele3d", lot });
    return { ...r.modele, triangles: new Float32Array(r.donnees) };
  }

  fermer(lot: number): Promise<void> {
    return this.appel({ op: "fermer", lot });
  }
}

export const moteur = new Moteur();
