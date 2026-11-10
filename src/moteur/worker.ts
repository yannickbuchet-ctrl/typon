/// <reference lib="webworker" />
/**
 * Web Worker du moteur : charge le module WebAssembly (Rust) et répond aux demandes de la page, pour que la lecture
 * d'un gros fichier ODB++ ne fige pas l'interface. Les fichiers restent dans ce navigateur.
 */
import urlWasm from "../wasm/typon_core.wasm?url";

interface Exports {
  memory: WebAssembly.Memory;
  alloc(n: number): number;
  dealloc(p: number, n: number): void;
  sortie_liberer(p: number): void;
  lot_nouveau(): number;
  lot_ajouter(lot: number, nom: number, nomN: number, donnees: number, donneesN: number): void;
  lot_analyser(lot: number): number;
  lot_fermer(lot: number): void;
  couche_geometrie(lot: number, couche: number): number;
  objet_sous(lot: number, couche: number, x: number, y: number, tol: number): number;
  zones_difference(masque: number, largeur: number, hauteur: number, fusion: number, minimum: number): number;
  modele_3d(lot: number): number;
}

let moteur: Promise<Exports> | null = null;

function charger(): Promise<Exports> {
  moteur ??= (async () => {
    const reponse = await fetch(urlWasm);
    const { instance } = await WebAssembly.instantiate(await reponse.arrayBuffer(), {});
    return instance.exports as unknown as Exports;
  })();
  return moteur;
}

const encodeur = new TextEncoder();
const decodeur = new TextDecoder();

/** Copie des octets dans la mémoire du moteur ; renvoie [pointeur, longueur]. */
function copier(m: Exports, octets: Uint8Array): [number, number] {
  const p = m.alloc(octets.length);
  new Uint8Array(m.memory.buffer, p, octets.length).set(octets);
  return [p, octets.length];
}

/** Lit une sortie du moteur (JSON + données binaires), puis la libère. */
function lire(m: Exports, p: number): { json: unknown; donnees: ArrayBuffer } {
  const vue = new DataView(m.memory.buffer);
  const total = vue.getUint32(p, true);
  const lj = vue.getUint32(p + 4, true);
  const json = JSON.parse(decodeur.decode(new Uint8Array(m.memory.buffer, p + 8, lj)));
  const debut = Math.ceil((8 + lj) / 4) * 4;
  const donnees = m.memory.buffer.slice(p + debut, p + total);
  m.sortie_liberer(p);
  return { json, donnees };
}

type Demande =
  | { op: "analyser"; fichiers: { nom: string; octets: ArrayBuffer }[] }
  | { op: "geometrie"; lot: number; couche: number }
  | { op: "objet"; lot: number; couche: number; x: number; y: number; tol: number }
  | { op: "zones"; masque: ArrayBuffer; largeur: number; hauteur: number; fusion: number; minimum: number }
  | { op: "modele3d"; lot: number }
  | { op: "fermer"; lot: number };

self.onmessage = async (e: MessageEvent<{ id: number } & Demande>) => {
  const { id, ...d } = e.data;
  try {
    const m = await charger();
    let resultat: unknown;
    const transferts: Transferable[] = [];
    switch (d.op) {
      case "analyser": {
        const lot = m.lot_nouveau();
        for (const f of d.fichiers) {
          const [pn, nn] = copier(m, encodeur.encode(f.nom));
          const [pd, nd] = copier(m, new Uint8Array(f.octets));
          m.lot_ajouter(lot, pn, nn, pd, nd);
          m.dealloc(pn, nn);
        }
        const t0 = performance.now();
        const { json } = lire(m, m.lot_analyser(lot));
        resultat = { lot, description: json, duree: performance.now() - t0 };
        break;
      }
      case "geometrie": {
        const { json, donnees } = lire(m, m.couche_geometrie(d.lot, d.couche));
        resultat = { geometrie: json, donnees };
        transferts.push(donnees);
        break;
      }
      case "objet":
        resultat = lire(m, m.objet_sous(d.lot, d.couche, d.x, d.y, d.tol)).json;
        break;
      case "zones": {
        const [p, n] = copier(m, new Uint8Array(d.masque));
        resultat = lire(m, m.zones_difference(p, d.largeur, d.hauteur, d.fusion, d.minimum)).json;
        m.dealloc(p, n);
        break;
      }
      case "modele3d": {
        const { json, donnees } = lire(m, m.modele_3d(d.lot));
        resultat = { modele: json, donnees };
        transferts.push(donnees);
        break;
      }
      case "fermer":
        m.lot_fermer(d.lot);
        resultat = null;
        break;
    }
    (self as unknown as Worker).postMessage({ id, ok: true, resultat }, transferts);
  } catch (err) {
    const message = err instanceof WebAssembly.RuntimeError
      ? "Le moteur s'est arrêté sur une erreur interne en lisant ces fichiers. Rechargez la page et réessayez, ou signalez le fichier en cause."
      : String((err as Error)?.message ?? err);
    // Après une erreur interne, le module n'est plus fiable : il sera rechargé à la prochaine demande.
    if (err instanceof WebAssembly.RuntimeError) moteur = null;
    (self as unknown as Worker).postMessage({ id, ok: false, erreur: message });
  }
};
