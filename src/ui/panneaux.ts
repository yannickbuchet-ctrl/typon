/**
 * Contenu du panneau de droite : inspecteur, résumé de la carte, fichiers et métadonnées, composants, nets,
 * nomenclature, perçages. Les données ODB++ sont mises en forme en tableaux filtrables et triables.
 */
import { t } from "../i18n";
import { traduireMoteur as tm } from "../i18n-moteur";
import type { DescriptionCouche } from "../moteur/client";
import type { DocumentTypon } from "./document";
import { el, longueur, nombre, taille, type Unite } from "./dom";
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

/** Liste de définitions ; `moteur` : libellés et valeurs venus du moteur, traduits à l'affichage. */
function definitions(lignes: [string, string | number | null | undefined][], moteur = false) {
  const dl = el("dl", { class: "definitions" });
  for (const [k, v] of lignes) {
    if (v === null || v === undefined || v === "") continue;
    dl.append(el("dt", { text: moteur ? tm(k) : k }), el("dd", { text: moteur ? tm(String(v)) : String(v) }));
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
    compte.textContent = l.length === lignes.length ? t("compte.lignes", { n: l.length }) : t("tableau.sur", { n: nombre(l.length, 0), total: nombre(lignes.length, 0) });
    if (l.length > limite) compte.textContent += t("tableau.affichees", { n: nombre(limite, 0) });
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
  if (lignes.length === 0) return el("p", { class: "vide", text: options.vide ?? t("tableau.vide") });
  if (options.filtre !== false && lignes.length > 8) {
    const champ = el("input", { type: "search", class: "filtre", placeholder: t("tableau.filtrer"), "aria-label": t("tableau.filtrerAria") });
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
  racine.append(section(t("resume.carte"), definitions([
    [t("resume.nom"), doc.nom],
    [t("resume.dimensions"), b ? `${longueur(b[2] - b[0], unite, 2)} × ${longueur(b[3] - b[1], unite, 2)}` : null],
    [t("resume.couchesCuivre"), d.meta.couchesCuivre || null],
    [t("resume.couchesLues"), d.couches.length],
    [t("resume.formats"), formats],
    [t("resume.logiciel"), logiciel],
    [t("resume.date"), date],
    [t("resume.composants"), d.meta.composants?.length || null],
    [t("resume.nets"), d.meta.nets?.length || null],
    [t("resume.taille"), taille(doc.taille)],
    [t("resume.lecture"), `${nombre(doc.duree / 1000, 2)} s`],
  ])));
  if (d.meta.job) {
    for (const r of d.meta.job.rubriques) racine.append(section(t("resume.rubriqueJob", { titre: tm(r.titre) }), definitions(r.lignes, true)));
    if (d.meta.job.empilage.length) {
      racine.append(section(t("resume.empilageJob"), tableau(d.meta.job.empilage, [
        { titre: t("resume.couche"), valeur: (l) => l.nom },
        { titre: t("resume.type"), valeur: (l) => l.type },
        { titre: t("resume.epaisseur"), valeur: (l) => l.epaisseur ?? "", nombre: true },
        { titre: t("resume.materiau"), valeur: (l) => l.materiau },
      ], { filtre: false })));
    }
  }
  if (d.meta.odb) {
    const source = d.meta.odb.source ?? "ODB++";
    racine.append(section(t("resume.fichierSource", { source }), definitions(d.meta.odb.info, true)));
    racine.append(section(t(source === "ODB++" ? "resume.matrice" : "resume.couchesEtape", { etape: d.meta.odb.etape }), tableau(d.meta.odb.empilage, [
      { titre: t("resume.couche"), valeur: (l) => l.nom },
      { titre: source === "ODB++" ? t("resume.typeOdb") : t("resume.fonction"), valeur: (l) => l.type },
      { titre: t("resume.role"), valeur: (l) => tm(l.role) },
      { titre: t("resume.deA"), valeur: (l) => (l.debut ? `${l.debut} → ${l.fin}` : "") },
      { titre: t("resume.elements"), valeur: (l) => l.elements, nombre: true },
    ])));
    if (d.meta.odb.etapes.length > 1) racine.append(section(t("resume.etapes"), el("p", { text: d.meta.odb.etapes.join(", ") })));
  }
  if (d.meta.netlist) racine.append(section(t("resume.netlist"), definitions([[t("resume.fichier"), d.meta.netlist.fichier], [t("resume.nets"), d.meta.netlist.nets]])));
  if (d.meta.listesOuvertures?.length) {
    racine.append(section(t("resume.listesOuvertures"), tableau(d.meta.listesOuvertures, [
      { titre: t("resume.fichier"), valeur: (l) => l.fichier },
      { titre: t("resume.ouvertures"), valeur: (l) => l.ouvertures, nombre: true },
      { titre: t("resume.unite"), valeur: (l) => tm(l.unite) },
    ], { filtre: false })));
  }
  const alertes = [...d.alertes, ...(d.meta.odb?.alertes ?? [])];
  if (alertes.length) racine.append(section(t("resume.remarques"), el("ul", { class: "alertes" }, ...alertes.map((a) => el("li", { html: icone("alerte", 14) }, el("span", { text: tm(a) }))))));
  if (d.ignores.length) racine.append(section(t("resume.nonLus"), el("p", { class: "discret", text: t("resume.nonLusTexte", { liste: d.ignores.join(", ") }) })));
  return racine;
}

/** Détails d'une couche : format, attributs X2, ouvertures, outils, commentaires, alertes. */
export function detailsCouche(c: DescriptionCouche, unite: Unite): HTMLElement {
  const m = c.meta;
  const racine = el("div", { class: "details-couche" });
  const source = c.role.source === "X2" ? t("couche.source.x2") : c.role.source === "ODB++" ? t("couche.source.odb") : t("couche.source.nom");
  racine.append(definitions([
    [t("couche.fichier"), c.fichier],
    [t("couche.format"), m.format ?? c.format],
    [t("couche.role"), t("couche.roleSource", { libelle: tm(c.role.libelle), source })],
    [t("couche.unite"), m.unite === "pouce" ? t("unite.pouces") : m.unite],
    [t("couche.notation"), m.notation && tm(m.notation)],
    [t("couche.objets"), c.objets],
    [t("couche.dimensions"), c.boite[2] > c.boite[0] ? `${longueur(c.boite[2] - c.boite[0], unite, 2)} × ${longueur(c.boite[3] - c.boite[1], unite, 2)}` : null],
    [t("couche.nets"), m.nets || null],
    [t("couche.composants"), m.composants || null],
    [t("couche.trous"), m.trous ?? null],
    [m.format === "IPC-2581" ? t("couche.fonctionIpc") : t("couche.typeOdb"), m.typeOdb],
    [m.format === "IPC-2581" ? t("couche.nomIpc") : t("couche.nomOdb"), m.nomOdb],
  ]));
  if (m.attributs?.length) racine.append(el("h4", { text: t("couche.attributs") }), definitions(m.attributs, true));
  if (m.outils?.length) {
    racine.append(el("h4", { text: t("couche.outils") }), tableau(m.outils, [
      { titre: t("couche.outil"), valeur: (o) => `T${String(o.numero).padStart(2, "0")}` },
      { titre: t("couche.diametre"), valeur: (o) => longueur(o.diametre, unite, 3) },
      { titre: t("couche.trous"), valeur: (o) => o.trous, nombre: true },
      { titre: t("couche.fraisages"), valeur: (o) => o.fraisages, nombre: true },
    ], { filtre: false }));
  }
  if (m.ouvertures?.length) {
    racine.append(el("h4", { text: t("couche.ouvertures") }), tableau(m.ouvertures, [
      { titre: t("couche.code"), valeur: (o) => o.code, largeur: "4.5em" },
      { titre: t("couche.forme"), valeur: (o) => tm(o.forme) },
      { titre: t("couche.fonctionX2"), valeur: (o) => o.fonction ?? "" },
    ]));
  }
  if (m.commentaires?.length) racine.append(el("h4", { text: t("couche.commentaires") }), el("pre", { class: "commentaires", text: m.commentaires.join("\n") }));
  if (m.alertes?.length) racine.append(el("h4", { text: t("couche.remarques") }), el("ul", { class: "alertes" }, ...m.alertes.map((a) => el("li", { text: tm(a) }))));
  return racine;
}

export function onglets(doc: DocumentTypon, unite: Unite, allerA: (x: number, y: number) => void): Onglet[] {
  const d = doc.description;
  const r: Onglet[] = [];
  const composants = d.meta.composants ?? [];
  if (composants.length) {
    r.push({ id: "composants", titre: t("onglet.composants"), icone: "puce", compte: composants.length, contenu: () => tableau(composants, [
      { titre: t("donnees.repere"), valeur: (c) => c.reference, largeur: "5em" },
      { titre: t("donnees.valeur"), valeur: (c) => c.valeur },
      { titre: t("donnees.boitier"), valeur: (c) => c.boitier },
      { titre: t("donnees.piece"), valeur: (c) => c.piece },
      { titre: t("donnees.cote"), valeur: (c) => tm(c.cote) },
      { titre: t("donnees.rotation"), valeur: (c) => (typeof c.rotation === "number" ? `${nombre(c.rotation, 1)}°` : c.rotation ? `${c.rotation}°` : "") },
    ], { cliquer: composants[0].x !== undefined ? (c) => allerA(c.x!, c.y!) : undefined, vide: t("donnees.aucunComposant") }) });
  }
  const bom = d.meta.nomenclature ?? [];
  if (bom.length) {
    r.push({ id: "nomenclature", titre: t("onglet.nomenclature"), icone: "liste", compte: bom.length, contenu: () => tableau(bom, [
      { titre: t("donnees.qte"), valeur: (l) => l.quantite, nombre: true, largeur: "3.5em" },
      { titre: t("donnees.piece"), valeur: (l) => l.piece },
      { titre: t("donnees.valeur"), valeur: (l) => l.valeur },
      { titre: t("donnees.reperes"), valeur: (l) => l.references.join(", ") },
    ]) });
  }
  const nets = d.meta.nets ?? [];
  if (nets.length) {
    const avecCompte = nets.some((n) => n.elements !== null);
    r.push({ id: "nets", titre: t("onglet.nets"), icone: "reseau", compte: nets.length, contenu: () => tableau(nets, [
      { titre: t("donnees.net"), valeur: (n) => n.nom },
      ...(avecCompte ? [{ titre: t("donnees.elements"), valeur: (n: { elements: number | null }) => n.elements ?? 0, nombre: true }] : []),
    ]) });
  }
  const outilsOdb = d.meta.odb?.outils ?? [];
  const percages = d.couches.filter((c) => c.meta.outils?.length);
  if (outilsOdb.length || percages.length) {
    r.push({ id: "percages", titre: t("onglet.percages"), icone: "percage", contenu: () => {
      const div = el("div");
      for (const p of percages) {
        div.append(section(tm(p.nom), tableau(p.meta.outils!, [
          { titre: t("couche.outil"), valeur: (o) => `T${String(o.numero).padStart(2, "0")}` },
          { titre: t("couche.diametre"), valeur: (o) => longueur(o.diametre, unite, 3) },
          { titre: t("couche.trous"), valeur: (o) => o.trous, nombre: true },
          { titre: t("couche.fraisages"), valeur: (o) => o.fraisages, nombre: true },
        ], { filtre: false })));
      }
      for (const p of outilsOdb) {
        div.append(section(`${p.couche}${p.debut ? ` (${p.debut} → ${p.fin})` : ""}`, tableau(p.outils, [
          { titre: t("couche.outil"), valeur: (o) => o.numero },
          { titre: t("donnees.type"), valeur: (o) => tm(o.genre) },
          { titre: t("donnees.percage"), valeur: (o) => longueur(o.percage, unite, 3) },
          { titre: t("donnees.fini"), valeur: (o) => longueur(o.fini, unite, 3) },
          { titre: t("couche.trous"), valeur: (o) => o.trous, nombre: true },
        ], { filtre: false })));
      }
      return div;
    } });
  }
  return r;
}
