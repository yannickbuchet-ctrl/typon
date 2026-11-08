/**
 * Contenu du panneau de droite : inspecteur, résumé de la carte, fichiers et métadonnées, composants, nets,
 * nomenclature, perçages. Les données ODB++ sont mises en forme en tableaux filtrables et triables.
 */
import type { DescriptionCouche } from "../moteur/client";
import type { DocumentTypon } from "./document";
import { el, longueur, nombre, pluriel, taille, type Unite } from "./dom";
import { icone } from "./icones";

export interface Onglet {
  id: string;
  titre: string;
  icone: string;
  compte?: number;
  contenu: () => HTMLElement;
}

function section(titre: string, ...contenu: (Node | string | null | false)[]) {
  return el("section", { class: "bloc" }, el("h3", { text: titre }), ...contenu);
}

function definitions(lignes: [string, string | number | null | undefined][]) {
  const dl = el("dl", { class: "definitions" });
  for (const [k, v] of lignes) {
    if (v === null || v === undefined || v === "") continue;
    dl.append(el("dt", { text: k }), el("dd", { text: String(v) }));
  }
  return dl;
}

/** Tableau triable avec filtre. `cliquer` : action sur une ligne (zoom sur un composant, par exemple). */
export function tableau<T>(lignes: T[], colonnes: { titre: string; valeur: (l: T) => string | number; nombre?: boolean; largeur?: string }[], options: { filtre?: boolean; cliquer?: (l: T) => void; vide?: string; limite?: number } = {}): HTMLElement {
  const conteneur = el("div", { class: "tableau" });
  let tri = -1;
  let inverse = false;
  let texte = "";
  const corps = el("tbody");
  const compte = el("span", { class: "compte" });
  const remplir = () => {
    let l = lignes;
    if (texte) {
      const t = texte.toLocaleLowerCase("fr");
      l = l.filter((x) => colonnes.some((c) => String(c.valeur(x)).toLocaleLowerCase("fr").includes(t)));
    }
    if (tri >= 0) {
      const c = colonnes[tri];
      l = [...l].sort((a, b) => {
        const va = c.valeur(a);
        const vb = c.valeur(b);
        const r = typeof va === "number" && typeof vb === "number" ? va - vb : String(va).localeCompare(String(vb), "fr", { numeric: true });
        return inverse ? -r : r;
      });
    }
    const limite = options.limite ?? 2000;
    corps.replaceChildren(...l.slice(0, limite).map((x) => {
      const tr = el("tr", options.cliquer ? { class: "cliquable", tabindex: "0", onclick: () => options.cliquer!(x), onkeydown: (e: KeyboardEvent) => e.key === "Enter" && options.cliquer!(x) } : {});
      for (const c of colonnes) {
        const v = c.valeur(x);
        tr.append(el("td", { class: c.nombre ? "nombre" : "", text: typeof v === "number" ? nombre(v, 3) : v }));
      }
      return tr;
    }));
    compte.textContent = l.length === lignes.length ? pluriel(l.length, "ligne") : `${nombre(l.length, 0)} sur ${nombre(lignes.length, 0)}`;
    if (l.length > limite) compte.textContent += ` (${nombre(limite, 0)} affichées)`;
  };
  const tete = el("tr");
  colonnes.forEach((c, i) => {
    const th = el("th", { class: c.nombre ? "nombre" : "", style: c.largeur ? { width: c.largeur } : {} }, el("button", { type: "button", text: c.titre, onclick: () => {
      inverse = tri === i ? !inverse : false;
      tri = i;
      tete.querySelectorAll("th").forEach((t, k) => t.setAttribute("aria-sort", k === i ? (inverse ? "descending" : "ascending") : "none"));
      remplir();
    } }));
    tete.append(th);
  });
  if (lignes.length === 0) return el("p", { class: "vide", text: options.vide ?? "Aucune donnée." });
  if (options.filtre !== false && lignes.length > 8) {
    const champ = el("input", { type: "search", class: "filtre", placeholder: "Filtrer…", "aria-label": "Filtrer le tableau" });
    champ.addEventListener("input", () => {
      texte = champ.value.trim();
      remplir();
    });
    conteneur.append(el("div", { class: "barre-tableau" }, champ, compte));
  } else {
    conteneur.append(el("div", { class: "barre-tableau" }, compte));
  }
  conteneur.append(el("div", { class: "defile" }, el("table", {}, el("thead", {}, tete), corps)));
  remplir();
  return conteneur;
}

/** Résumé de la carte : dimensions, couches, logiciel, fabrication. */
export function resume(doc: DocumentTypon, unite: Unite): HTMLElement {
  const d = doc.description;
  const b = doc.boite;
  const racine = el("div");
  const formats = [...new Set(d.couches.map((c) => c.meta.format ?? c.format))].join(", ");
  const logiciel = d.couches.flatMap((c) => c.meta.attributs ?? []).find(([k]) => k === "Logiciel")?.[1]
    ?? d.meta.odb?.info.find(([k]) => k === "Logiciel d'origine")?.[1];
  const date = d.couches.flatMap((c) => c.meta.attributs ?? []).find(([k]) => k === "Date de création")?.[1]
    ?? d.meta.odb?.info.find(([k]) => k === "Date de création")?.[1];
  racine.append(section("Carte", definitions([
    ["Nom", doc.nom],
    ["Dimensions", b ? `${longueur(b[2] - b[0], unite, 2)} × ${longueur(b[3] - b[1], unite, 2)}` : null],
    ["Couches de cuivre", d.meta.couchesCuivre || null],
    ["Couches lues", d.couches.length],
    ["Formats", formats],
    ["Logiciel d'origine", logiciel],
    ["Date de création", date],
    ["Composants", d.meta.composants?.length || null],
    ["Nets", d.meta.nets?.length || null],
    ["Taille des fichiers", taille(doc.taille)],
    ["Lecture", `${nombre(doc.duree / 1000, 2)} s`],
  ])));
  if (d.meta.job) {
    for (const r of d.meta.job.rubriques) racine.append(section(`${r.titre} (fichier de job)`, definitions(r.lignes)));
    if (d.meta.job.empilage.length) {
      racine.append(section("Empilage (fichier de job)", tableau(d.meta.job.empilage, [
        { titre: "Couche", valeur: (l) => l.nom },
        { titre: "Type", valeur: (l) => l.type },
        { titre: "Épaisseur (mm)", valeur: (l) => l.epaisseur ?? "", nombre: true },
        { titre: "Matériau", valeur: (l) => l.materiau },
      ], { filtre: false })));
    }
  }
  if (d.meta.odb) {
    racine.append(section("Fichier ODB++", definitions(d.meta.odb.info)));
    racine.append(section(`Matrice des couches (étape « ${d.meta.odb.etape} »)`, tableau(d.meta.odb.empilage, [
      { titre: "Couche", valeur: (l) => l.nom },
      { titre: "Type ODB++", valeur: (l) => l.type },
      { titre: "Rôle", valeur: (l) => l.role },
      { titre: "De → à", valeur: (l) => (l.debut ? `${l.debut} → ${l.fin}` : "") },
      { titre: "Éléments", valeur: (l) => l.elements, nombre: true },
    ])));
    if (d.meta.odb.etapes.length > 1) racine.append(section("Étapes", el("p", { text: d.meta.odb.etapes.join(", ") })));
  }
  if (d.meta.netlist) racine.append(section("Netlist IPC-D-356", definitions([["Fichier", d.meta.netlist.fichier], ["Nets", d.meta.netlist.nets]])));
  const alertes = [...d.alertes, ...(d.meta.odb?.alertes ?? [])];
  if (alertes.length) racine.append(section("Remarques", el("ul", { class: "alertes" }, ...alertes.map((a) => el("li", { html: icone("alerte", 14) }, el("span", { text: a }))))));
  if (d.ignores.length) racine.append(section("Fichiers non lus", el("p", { class: "discret", text: `Ces fichiers ne sont ni des Gerber, ni des perçages, ni de l'ODB++ : ${d.ignores.join(", ")}.` })));
  return racine;
}

/** Détails d'une couche : format, attributs X2, ouvertures, outils, commentaires, alertes. */
export function detailsCouche(c: DescriptionCouche, unite: Unite): HTMLElement {
  const m = c.meta;
  const racine = el("div", { class: "details-couche" });
  racine.append(definitions([
    ["Fichier", c.fichier],
    ["Format", m.format ?? c.format],
    ["Rôle", `${c.role.libelle} (d'après ${c.role.source === "X2" ? "les attributs X2" : c.role.source === "ODB++" ? "la matrice ODB++" : "le nom du fichier"})`],
    ["Unité du fichier", m.unite === "pouce" ? "pouces" : m.unite],
    ["Notation des coordonnées", m.notation],
    ["Objets", c.objets],
    ["Dimensions", c.boite[2] > c.boite[0] ? `${longueur(c.boite[2] - c.boite[0], unite, 2)} × ${longueur(c.boite[3] - c.boite[1], unite, 2)}` : null],
    ["Nets", m.nets || null],
    ["Composants", m.composants || null],
    ["Trous", m.trous ?? null],
    ["Type ODB++", m.typeOdb],
    ["Nom ODB++", m.nomOdb],
  ]));
  if (m.attributs?.length) racine.append(el("h4", { text: "Attributs du fichier (X2)" }), definitions(m.attributs));
  if (m.outils?.length) {
    racine.append(el("h4", { text: "Outils de perçage" }), tableau(m.outils, [
      { titre: "Outil", valeur: (o) => `T${String(o.numero).padStart(2, "0")}` },
      { titre: "Diamètre", valeur: (o) => longueur(o.diametre, unite, 3) },
      { titre: "Trous", valeur: (o) => o.trous, nombre: true },
      { titre: "Fraisages", valeur: (o) => o.fraisages, nombre: true },
    ], { filtre: false }));
  }
  if (m.ouvertures?.length) {
    racine.append(el("h4", { text: "Ouvertures (apertures)" }), tableau(m.ouvertures, [
      { titre: "Code", valeur: (o) => o.code, largeur: "4.5em" },
      { titre: "Forme", valeur: (o) => o.forme },
      { titre: "Fonction (X2)", valeur: (o) => o.fonction ?? "" },
    ]));
  }
  if (m.commentaires?.length) racine.append(el("h4", { text: "Commentaires du fichier" }), el("pre", { class: "commentaires", text: m.commentaires.join("\n") }));
  if (m.alertes?.length) racine.append(el("h4", { text: "Remarques" }), el("ul", { class: "alertes" }, ...m.alertes.map((a) => el("li", { text: a }))));
  return racine;
}

export function onglets(doc: DocumentTypon, unite: Unite, allerA: (x: number, y: number) => void): Onglet[] {
  const d = doc.description;
  const r: Onglet[] = [];
  const composants = d.meta.composants ?? [];
  if (composants.length) {
    r.push({ id: "composants", titre: "Composants", icone: "puce", compte: composants.length, contenu: () => tableau(composants, [
      { titre: "Repère", valeur: (c) => c.reference, largeur: "5em" },
      { titre: "Valeur", valeur: (c) => c.valeur },
      { titre: "Boîtier", valeur: (c) => c.boitier },
      { titre: "Pièce", valeur: (c) => c.piece },
      { titre: "Côté", valeur: (c) => c.cote },
      { titre: "Rotation", valeur: (c) => (typeof c.rotation === "number" ? `${nombre(c.rotation, 1)}°` : c.rotation ? `${c.rotation}°` : "") },
    ], { cliquer: composants[0].x !== undefined ? (c) => allerA(c.x!, c.y!) : undefined, vide: "Aucun composant." }) });
  }
  const bom = d.meta.nomenclature ?? [];
  if (bom.length) {
    r.push({ id: "nomenclature", titre: "Nomenclature", icone: "liste", compte: bom.length, contenu: () => tableau(bom, [
      { titre: "Qté", valeur: (l) => l.quantite, nombre: true, largeur: "3.5em" },
      { titre: "Pièce", valeur: (l) => l.piece },
      { titre: "Valeur", valeur: (l) => l.valeur },
      { titre: "Repères", valeur: (l) => l.references.join(", ") },
    ]) });
  }
  const nets = d.meta.nets ?? [];
  if (nets.length) {
    const avecCompte = nets.some((n) => n.elements !== null);
    r.push({ id: "nets", titre: "Nets", icone: "reseau", compte: nets.length, contenu: () => tableau(nets, [
      { titre: "Net", valeur: (n) => n.nom },
      ...(avecCompte ? [{ titre: "Éléments", valeur: (n: { elements: number | null }) => n.elements ?? 0, nombre: true }] : []),
    ]) });
  }
  const outilsOdb = d.meta.odb?.outils ?? [];
  const percages = d.couches.filter((c) => c.meta.outils?.length);
  if (outilsOdb.length || percages.length) {
    r.push({ id: "percages", titre: "Perçages", icone: "percage", contenu: () => {
      const div = el("div");
      for (const p of percages) {
        div.append(section(p.nom, tableau(p.meta.outils!, [
          { titre: "Outil", valeur: (o) => `T${String(o.numero).padStart(2, "0")}` },
          { titre: "Diamètre", valeur: (o) => longueur(o.diametre, unite, 3) },
          { titre: "Trous", valeur: (o) => o.trous, nombre: true },
          { titre: "Fraisages", valeur: (o) => o.fraisages, nombre: true },
        ], { filtre: false })));
      }
      for (const p of outilsOdb) {
        div.append(section(`${p.couche}${p.debut ? ` (${p.debut} → ${p.fin})` : ""}`, tableau(p.outils, [
          { titre: "Outil", valeur: (o) => o.numero },
          { titre: "Type", valeur: (o) => o.genre },
          { titre: "Perçage", valeur: (o) => longueur(o.percage, unite, 3) },
          { titre: "Fini", valeur: (o) => longueur(o.fini, unite, 3) },
          { titre: "Trous", valeur: (o) => o.trous, nombre: true },
        ], { filtre: false })));
      }
      return div;
    } });
  }
  return r;
}
