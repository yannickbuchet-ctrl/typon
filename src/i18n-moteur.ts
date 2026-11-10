/**
 * Traduction, au moment de l'affichage, des textes français renvoyés par le moteur Rust (noms et rôles des couches,
 * libellés des propriétés, métadonnées, alertes, messages d'erreur). Le moteur reste en français : l'interface s'appuie
 * sur ses libellés (« Logiciel », « Perçage métallisé »…) pour sa logique, et ne les traduit qu'à l'écran.
 *
 * Trois étapes : table des libellés fixes, découpage des libellés composés (« Composant · valeur »), puis motifs pour
 * les messages paramétrés. Un texte inconnu est rendu tel quel.
 */
import { langue } from "./i18n";

/** Libellés fixes du moteur (français → anglais). */
const FIXES: Record<string, string> = {
  // Couches (couches.rs, lib.rs, odb.rs, ipc2581.rs)
  "Cuivre": "Copper",
  "Masque de soudure": "Solder mask",
  "Sérigraphie": "Silkscreen",
  "Pâte à braser": "Solder paste",
  "Contour de la carte": "Board outline",
  "Perçage": "Drill",
  "Fraisage": "Rout",
  "Composants": "Components",
  "Mécanique": "Mechanical",
  "Plan de perçage": "Drill map",
  "Plan de fabrication": "Fabrication drawing",
  "Plan d'assemblage": "Assembly drawing",
  "Plan du panneau": "Panel drawing",
  "Colle": "Glue",
  "Netlist": "Netlist",
  "Fichier de job": "Job file",
  "Autre": "Other",
  "Contour": "Outline",
  // Côtés, oui / non, métallisation
  "dessus": "top",
  "dessous": "bottom",
  "interne": "inner",
  "oui": "yes",
  "non": "no",
  "métallisé": "plated",
  "non métallisé": "non-plated",
  "via": "via",
  // Unités
  "pouce": "inches",
  "pouces": "inches",
  "millimètres": "millimeters",
  "microns": "microns",
  // Inspecteur : propriétés des objets (gerber.rs, excellon.rs, odb.rs, ipc2581.rs)
  "Objet": "Object",
  "Ouverture": "Aperture",
  "Polarité": "Polarity",
  "Net": "Net",
  "Broche": "Pin",
  "Composant": "Component",
  "Fonction": "Function",
  "Tolérance de perçage": "Drill tolerance",
  "Texte": "Text",
  "rotation": "rotation",
  "fabricant": "manufacturer",
  "référence fabricant": "manufacturer part number",
  "valeur": "value",
  "montage": "mounting",
  "empreinte": "footprint",
  "boîtier": "package",
  "description du boîtier": "package description",
  "hauteur": "height",
  "bibliothèque": "library",
  "description": "description",
  "fournisseur": "supplier",
  "fonction": "function",
  "Outil": "Tool",
  "Diamètre": "Diameter",
  "Référence": "Reference",
  "Pièce": "Part",
  "Boîtier": "Package",
  "Position": "Position",
  "Rotation": "Rotation",
  "Côté": "Side",
  "Valeur": "Value",
  "Symbole": "Symbol",
  "Code-barres": "Barcode",
  "Usage de la pastille": "Pad usage",
  "CMS": "SMD",
  "Géométrie": "Geometry",
  "Point de test": "Test point",
  "Nomenclature": "BOM",
  "Métallisation": "Plating",
  "Usage": "Usage",
  "Pastille": "Pad",
  "Trait": "Line",
  "Arc": "Arc",
  "Surface": "Surface",
  "Profil": "Profile",
  "Région": "Region",
  "Flash": "Flash",
  "Élément": "Feature",
  "Trou": "Hole",
  "Rainure": "Slot",
  "Contour de la carte (profil ODB++)": "Board outline (ODB++ profile)",
  "Contour de la carte (profil IPC-2581)": "Board outline (IPC-2581 profile)",
  "claire (efface)": "clear (erases)",
  "connexion (toeprint)": "connection (toeprint)",
  "pastille de repère (fiducial)": "fiducial pad",
  "pastille isolée": "isolated pad",
  "pastille sans usage": "unused pad",
  "bloc d'ouverture": "aperture block",
  "bloc d'ouverture (AB)": "aperture block (AB)",
  // Attributs X2 (lib.rs)
  "Fonction du fichier": "File function",
  "Logiciel": "Software",
  "Date de création": "Creation date",
  "Projet": "Project",
  "Partie": "Part",
  "Mêmes coordonnées que": "Same coordinates as",
  "Empreinte MD5": "MD5 checksum",
  "Fonction de l'ouverture": "Aperture function",
  "positive": "positive",
  "négative": "negative",
  "carte seule": "single board",
  "panneau": "panel",
  "panneau de fabrication": "fabrication panel",
  "coupon de test": "test coupon",
  // Fichier de job (job.rs)
  "En-tête": "Header",
  "Caractéristiques générales": "General specifications",
  "Règles de conception": "Design rules",
  "Éditeur": "Vendor",
  "Application": "Application",
  "Version": "Version",
  "Nom": "Name",
  "Identifiant": "Identifier",
  "Révision": "Revision",
  "Taille": "Size",
  "largeur": "width",
  "Nombre de couches": "Number of layers",
  "Épaisseur": "Thickness",
  "Finition": "Finish",
  "Impédance contrôlée": "Controlled impedance",
  "couleur": "color",
  "Couches": "Layers",
  "Écart pastille-pastille": "Pad to pad clearance",
  "Écart pastille-piste": "Pad to track clearance",
  "Écart piste-piste": "Track to track clearance",
  "Largeur de piste minimale": "Minimum track width",
  "Écart piste-zone": "Track to region clearance",
  "Écart zone-zone": "Region to region clearance",
  "Classe IPC": "IPC class",
  "Couches externes": "Outer layers",
  "Couches internes": "Inner layers",
  "Toutes les couches": "All layers",
  // Informations ODB++ (odb.rs) et IPC-2581 (ipc2581.rs)
  "Nom du produit": "Product name",
  "Version ODB++ (majeure)": "ODB++ version (major)",
  "Version ODB++ (mineure)": "ODB++ version (minor)",
  "Logiciel d'origine": "Source software",
  "Dernier enregistrement": "Last saved",
  "Enregistré avec": "Saved with",
  "Enregistré par": "Saved by",
  "Unités": "Units",
  "Identifiant maximal": "Maximum UID",
  "Format": "Format",
  "Mode": "Mode",
  "Conception": "Design",
  "Entreprise": "Company",
  "Auteur": "Author",
  "Dernière modification": "Last modified",
  "Épaisseur de l'empilage": "Stackup thickness",
  "Épaisseur totale": "Total thickness",
  "fabrication": "fabrication",
  "assemblage": "assembly",
  "test": "test",
  "empilage": "stackup",
  "conception": "design",
  "défini par l'utilisateur": "user-defined",
  "complet": "full",
  // Alertes et erreurs sans paramètre
  "Image négative (IP NEG, commande ancienne) : le fichier décrit les zones sans cuivre.": "Negative image (IP NEG, legacy command): the file describes the areas without copper.",
  "Coordonnées incrémentales (FSI, ancienne notation).": "Incremental coordinates (FSI, legacy notation).",
  "Tracé sans ouverture sélectionnée : ignoré.": "Draw with no aperture selected: ignored.",
  "Flash sans ouverture sélectionnée : ignoré.": "Flash with no aperture selected: ignored.",
  "Ni format (FS) ni unité (MO) : fichier RS-274D sans sa liste d'ouvertures. Déposez-la avec lui (.whl, .apr, liste de D-codes…) pour un dessin juste.": "Neither format (FS) nor unit (MO): RS-274D file without its aperture list. Drop the list with it (.whl, .apr, D-code list…) for an accurate drawing.",
  "Unité non indiquée : pouces supposés (valeur par défaut d'Excellon).": "Unit not specified: inches assumed (Excellon default).",
  "Police fonts/standard absente : les textes ne sont pas dessinés.": "Font fonts/standard missing: texts are not drawn.",
  "Pas de fichier matrix/matrix : ce n'est pas un produit ODB++.": "No matrix/matrix file: this is not an ODB++ product.",
  "Fichier matrix/matrix illisible.": "Unreadable matrix/matrix file.",
  "Ce fichier XML n'est pas un fichier IPC-2581.": "This XML file is not an IPC-2581 file.",
  "Fichier IPC-2581 sans étape (Step).": "IPC-2581 file without a step (Step).",
  "Archive trop volumineuse une fois décompressée (plus de 1 Go).": "Archive too large once decompressed (more than 1 GB).",
  "Archive ZIP abîmée (répertoire introuvable).": "Damaged ZIP archive (central directory not found).",
  "Archive ZIP abîmée (entrée illisible).": "Damaged ZIP archive (unreadable entry).",
  "Archive ZIP tronquée.": "Truncated ZIP archive.",
  "Fichier gzip tronqué.": "Truncated gzip file.",
  "Décompression gzip impossible (fichier abîmé ?).": "Gzip decompression failed (damaged file?).",
  "Archive TAR tronquée.": "Truncated TAR archive.",
  "Fichier .Z tronqué.": "Truncated .Z file.",
  "Fichier .Z abîmé.": "Damaged .Z file.",
  "Fichier .Z trop volumineux.": ".Z file too large.",
  "Lot inconnu.": "Unknown file set.",
  "Couche inconnue.": "Unknown layer.",
  // Messages du Web Worker (src/moteur)
  "Le moteur s'est arrêté sur une erreur interne en lisant ces fichiers. Rechargez la page et réessayez, ou signalez le fichier en cause.": "The engine stopped on an internal error while reading these files. Reload the page and try again, or report the file involved.",
  "Le moteur n'a pas pu démarrer.": "The engine could not start.",
};

/** Bases des noms de couches, pour « Masque de soudure dessus » → « Top solder mask ». */
const BASES: Record<string, string> = {
  "Cuivre": "copper",
  "Masque de soudure": "solder mask",
  "Sérigraphie": "silkscreen",
  "Pâte à braser": "solder paste",
  "Perçage": "drill",
  "Fraisage": "rout",
  "Composants": "components",
  "Mécanique": "mechanical",
  "Plan de perçage": "drill map",
  "Plan d'assemblage": "assembly drawing",
  "Colle": "glue",
};
const COTES: Record<string, string> = { dessus: "Top", dessous: "Bottom", interne: "Inner" };

/** Nombres écrits à la française par le moteur (« 1,6 mm ») : point décimal en anglais. */
const point = (s: string) => s.replace(/(\d),(\d)/g, "$1.$2");
const guillemets = (s: string) => `“${s}”`;
const echapper = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const RE_BASES = Object.keys(BASES).sort((a, b) => b.length - a.length).map(echapper).join("|");

/** Motifs des textes paramétrés : [expression, traduction]. */
const MOTIFS: [RegExp, (...g: string[]) => string][] = [
  // Couches
  [/^Cuivre interne (\d+)$/, (n) => `Inner copper ${n}`],
  [new RegExp(`^(${RE_BASES}) (dessus|dessous|interne)$`), (b, c) => `${COTES[c]} ${BASES[b]}`],
  [/^(Perçage|Fraisage) (non métallisé|métallisé)(.*)$/, (b, m, reste) => `${m === "métallisé" ? "Plated" : "Non-plated"} ${b === "Perçage" ? "drill" : "rout"}${traduireMoteur(reste)}`],
  [/^ (.+?) \(avec fraisages\)$/, (e) => ` ${e} (with routs)`],
  [/^ (\S+–\S+)$/, (e) => ` ${e}`],
  [/^ « (.+) »$/, (nom) => ` ${guillemets(nom)}`],
  [/^(.+) « (.+) »$/, (base, nom) => `${traduireMoteur(base)} ${guillemets(nom)}`],
  [/^(\S+) (dessus|dessous)$/, (nom, c) => `${COTES[c]} ${nom}`],
  // Propriétés et valeurs
  [/^X (.+) ; Y (.+) mm$/, (x, y) => `X ${point(x)}; Y ${point(y)} mm`],
  [/^(.+)° \(sens trigonométrique\)$/, (r) => `${point(r)}° (counterclockwise)`],
  [/^code 39 « (.*) »$/, (texte) => `code 39 ${guillemets(texte)}`],
  [/^D(\d+) : (.+)$/, (n, forme) => `D${n}: ${traduireMoteur(forme)}`],
  [/^cercle (Ø.+?)(?:, trou (Ø.+))?$/, (d, trou) => `circle ${point(d)}${trou ? `, hole ${point(trou)}` : ""}`],
  [/^rectangle (.+)$/, (d) => `rectangle ${point(d)}`],
  [/^oblong (.+)$/, (d) => `obround ${point(d)}`],
  [/^polygone à (\d+) côtés, (Ø.+)$/, (n, d) => `polygon with ${n} sides, ${point(d)}`],
  [/^macro (.+)$/, (d) => `macro ${point(d)}`],
  [/^IPC-2581 révision (.+)$/, (r) => `IPC-2581 revision ${r}`],
  [/^-?\d+(,\d+)?( mm)?$/, () => ""],
  // Notation des coordonnées (gerber.rs, excellon.rs)
  [/^(\d+\.\d+)(, zéros de tête présents|, zéros de fin présents)?( \(supposé\))?( \(zéros de fin omis\))?$/, (f, z, s, o) =>
    `${f}${z ? (z.includes("tête") ? ", leading zeros present" : ", trailing zeros present") : ""}${s ? " (assumed)" : ""}${o ? " (trailing zeros omitted)" : ""}`],
  // Alertes et erreurs
  [/^Gerber RS-274D : ouvertures lues dans « (.+) » \(dimensions en (\S+)\) ; coordonnées supposées au format (\S+) en (\S+), (zéros de tête omis|zéros de fin omis)\. Vérifiez les dimensions de la carte\.$/,
    (liste, u1, f, u2, z) => `Gerber RS-274D: apertures read from ${guillemets(liste)} (dimensions in ${traduireMoteur(u1)}); coordinates assumed to be in ${f} format in ${traduireMoteur(u2)}, ${z.includes("tête") ? "leading" : "trailing"} zeros omitted. Check the board dimensions.`],
  [/^Commande ancienne %(.+)% ignorée : le dessin peut être décalé ou tourné\.$/, (c) => `Legacy command %${c}% ignored: the drawing may be offset or rotated.`],
  [/^Macro d'ouverture « (.+) » inconnue \(D(\d+)\)\.$/, (m, n) => `Unknown aperture macro ${guillemets(m)} (D${n}).`],
  [/^Tracé avec l'ouverture D(\d+), qui n'est ni un cercle ni un rectangle : dessiné en trait fin\.$/, (n) => `Draw with aperture D${n}, which is neither a circle nor a rectangle: drawn as a thin line.`],
  [/^Ouverture D(\d+) non définie : tracés dessinés en trait fin \(fichier RS-274D sans ses ouvertures \?\)\.$/, (n) => `Aperture D${n} not defined: draws rendered as thin lines (RS-274D file without its apertures?).`],
  [/^Ouverture D(\d+) non définie : flashs dessinés en point \(fichier RS-274D sans ses ouvertures \?\)\.$/, (n) => `Aperture D${n} not defined: flashes rendered as dots (RS-274D file without its apertures?).`],
  [/^Symbole « (.+) » inconnu : dessiné comme un petit disque\.$/, (s) => `Unknown symbol ${guillemets(s)}: drawn as a small disc.`],
  [/^Primitive IPC-2581 « (.+) » non prise en charge\.$/, (p) => `IPC-2581 primitive ${guillemets(p)} not supported.`],
  [/^Primitive « (.+) » absente du dictionnaire\.$/, (p) => `Primitive ${guillemets(p)} missing from the dictionary.`],
  [/^(\d+) texte\(s\) IPC-2581 \(élément Text\) non dessiné\(s\)\.$/, (n) => `${n} IPC-2581 text(s) (Text element) not drawn.`],
  [/^Fichier IPC-2581 illisible : (.+)$/, (e) => `Unreadable IPC-2581 file: ${e}`],
  [/^Fichier de job illisible : (.+)$/, (e) => `Unreadable job file: ${e}`],
  [/^« (.+) » est chiffré dans l'archive : ouvrez-la et décompressez-la d'abord\.$/, (n) => `${guillemets(n)} is encrypted in the archive: open and extract it first.`],
  [/^« (.+) » : décompression impossible\.$/, (n) => `${guillemets(n)}: decompression failed.`],
  [/^« (.+) » : méthode de compression (\d+) non prise en charge\.$/, (n, m) => `${guillemets(n)}: compression method ${m} not supported.`],
];

/** Traduit un texte du moteur dans la langue de l'interface (rendu tel quel en français, ou s'il est inconnu). */
export function traduireMoteur(texte: string): string {
  if (langue() === "fr" || !texte) return texte;
  return traduire(texte);
}

function traduire(texte: string): string {
  const fixe = FIXES[texte];
  if (fixe !== undefined) return fixe;
  // Libellés composés : « Composant · valeur », « Règles de conception · Couches externes · Écart piste-piste ».
  if (texte.includes(" · ")) return texte.split(" · ").map(traduire).join(" · ");
  for (const [re, f] of MOTIFS) {
    const m = texte.match(re);
    if (m) return f(...m.slice(1)) || point(texte);
  }
  // Nom de couche en double, suivi de son fichier : « Cuivre dessus (carte-F_Cu.gbr) ».
  const double = texte.match(/^(.+) \(([^()]+)\)$/);
  if (double) {
    const base = traduire(double[1]);
    if (base !== double[1]) return `${base} (${double[2]})`;
  }
  // Message précédé d'un nom de fichier : « carte.zip : Archive ZIP tronquée. »
  for (let i = texte.indexOf(" : "); i > 0; i = texte.indexOf(" : ", i + 3)) {
    const suite = texte.slice(i + 3);
    const traduite = traduire(suite);
    if (traduite !== suite) return `${texte.slice(0, i)}: ${traduite}`;
  }
  return texte;
}
