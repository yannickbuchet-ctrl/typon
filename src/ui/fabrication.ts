/**
 * Outils de fabrication : surlignage et longueur d'un net, vérifications DFM (profils de fabricants, liste des défauts
 * cliquable, marques sur la carte, rapport PDF), et mise en panneau (Gerber X2 et Excellon dans un ZIP, rouvert dans
 * Typon pour l'aperçu).
 */
import { langue, locale } from "../i18n";
import { traduireMoteur as tm } from "../i18n-moteur";
import { moteur, type DefautDfm, type InfoNet, type ResultatDfm } from "../moteur/client";
import type { CoucheAffichee, CoucheGpu, Rendu } from "../rendu/rendu";
import type { DocumentTypon } from "./document";
import { el, longueur, nombre, telecharger, type Unite } from "./dom";
import { icone } from "./icones";

/** Texte selon la langue de l'interface. */
const L = (fr: string, en: string) => (langue() === "en" ? en : fr);

export interface Hote {
  doc(): DocumentTypon | null;
  unite(): Unite;
  rendu(): Rendu;
  allerA(x: number, y: number, taille?: number): void;
  ajuster(boite: [number, number, number, number]): void;
  redessiner(): void;
  message(texte: string, genre?: "info" | "erreur" | "succes"): void;
  attendre<T>(texte: string, travail: () => Promise<T>): Promise<T | undefined>;
  ouvrirFichiers(fichiers: File[], role: "A" | "B", nom?: string): Promise<void>;
  montrerOnglet(id: string): void;
}

/* --- Règles DFM ------------------------------------------------------------------------------------------------- */

type CleRegle = "piste" | "isolement" | "anneau" | "percage" | "trouTrou" | "cuivreBord" | "cuivreNpth" | "pontVernis" | "serigraphie";
const SEUILS: CleRegle[] = ["piste", "isolement", "anneau", "percage", "trouTrou", "cuivreBord", "cuivreNpth", "pontVernis", "serigraphie"];

/** Profils indicatifs (mm), d'après les capacités publiées des fabricants pour une carte standard. */
const PROFILS: { id: string; nom: string; seuils: Record<CleRegle, number> }[] = [
  { id: "jlcpcb", nom: "JLCPCB (standard)", seuils: { piste: 0.127, isolement: 0.127, anneau: 0.13, percage: 0.3, trouTrou: 0.25, cuivreBord: 0.3, cuivreNpth: 0.2, pontVernis: 0.1, serigraphie: 0.15 } },
  { id: "pcbway", nom: "PCBWay (standard)", seuils: { piste: 0.1, isolement: 0.1, anneau: 0.15, percage: 0.2, trouTrou: 0.2, cuivreBord: 0.25, cuivreNpth: 0.2, pontVernis: 0.1, serigraphie: 0.15 } },
  { id: "eurocircuits", nom: "Eurocircuits (classe 6)", seuils: { piste: 0.15, isolement: 0.15, anneau: 0.15, percage: 0.25, trouTrou: 0.25, cuivreBord: 0.25, cuivreNpth: 0.25, pontVernis: 0.1, serigraphie: 0.15 } },
  { id: "oshpark", nom: "OSH Park (2 couches)", seuils: { piste: 0.152, isolement: 0.152, anneau: 0.178, percage: 0.254, trouTrou: 0.25, cuivreBord: 0.381, cuivreNpth: 0.2, pontVernis: 0.1, serigraphie: 0.15 } },
  { id: "prudent", nom: L("Prudent (toute usine)", "Conservative (any fab)"), seuils: { piste: 0.2, isolement: 0.2, anneau: 0.2, percage: 0.3, trouTrou: 0.3, cuivreBord: 0.5, cuivreNpth: 0.3, pontVernis: 0.15, serigraphie: 0.15 } },
];

type Gravite = "erreur" | "avertissement" | "info";
const REGLES: Record<string, { fr: string; en: string; seuil?: CleRegle; gravite: Gravite; surface?: boolean }> = {
  court: { fr: "Court-circuit possible", en: "Possible short", gravite: "erreur" },
  isolement: { fr: "Isolement cuivre", en: "Copper clearance", seuil: "isolement", gravite: "erreur" },
  piste: { fr: "Largeur de piste", en: "Track width", seuil: "piste", gravite: "erreur" },
  anneau: { fr: "Anneau annulaire", en: "Annular ring", seuil: "anneau", gravite: "erreur" },
  pastilleAbsente: { fr: "Pastille absente", en: "Missing pad", gravite: "erreur" },
  percage: { fr: "Perçage minimum", en: "Minimum drill", seuil: "percage", gravite: "erreur" },
  trouTrou: { fr: "Écart entre trous", en: "Hole-to-hole spacing", seuil: "trouTrou", gravite: "avertissement" },
  cuivreBord: { fr: "Cuivre au bord de la carte", en: "Copper to board edge", seuil: "cuivreBord", gravite: "erreur" },
  cuivreNpth: { fr: "Cuivre près d'un trou non métallisé", en: "Copper to NPTH", seuil: "cuivreNpth", gravite: "avertissement" },
  pontVernis: { fr: "Pont de vernis", en: "Solder mask bridge", seuil: "pontVernis", gravite: "avertissement" },
  serigraphie: { fr: "Trait de sérigraphie trop fin", en: "Thin silkscreen line", seuil: "serigraphie", gravite: "info" },
  serigraphiePastille: { fr: "Sérigraphie sur une pastille", en: "Silkscreen on pad", gravite: "info", surface: true },
  serigraphieHors: { fr: "Sérigraphie hors de la carte", en: "Silkscreen off board", gravite: "info", surface: true },
};
const ORDRE = Object.keys(REGLES);
const nomRegle = (r: string) => (REGLES[r] ? L(REGLES[r].fr, REGLES[r].en) : r);
const COULEURS: Record<Gravite, string> = { erreur: "#ff5a5a", avertissement: "#ffb347", info: "#ffd166" };

const STOCKAGE = "typon-dfm";

export class Fabrication {
  private net: { nom: string; info: InfoNet; couches: Map<number, CoucheGpu> } | null = null;
  private dfm: { resultat: ResultatDfm; regle: string | null; choisi: number } | null = null;
  private profil = "jlcpcb";
  private seuils: Record<CleRegle, number> = { ...PROFILS[0].seuils };

  constructor(private hote: Hote) {
    try {
      const s = JSON.parse(localStorage.getItem(STOCKAGE) ?? "null");
      if (s?.profil) this.profil = s.profil;
      if (s?.seuils) for (const k of SEUILS) if (typeof s.seuils[k] === "number") this.seuils[k] = s.seuils[k];
    } catch {
      /* stockage indisponible */
    }
  }

  private memoriser() {
    try {
      localStorage.setItem(STOCKAGE, JSON.stringify({ profil: this.profil, seuils: this.seuils }));
    } catch {
      /* stockage indisponible */
    }
  }

  /** Le document a changé : tout est effacé. */
  effacer() {
    if (this.net) for (const g of this.net.couches.values()) this.hote.rendu().liberer(g);
    this.net = null;
    this.dfm = null;
  }

  get netSurligne(): string | null {
    return this.net?.nom ?? null;
  }

  get aDesResultats(): boolean {
    return !!this.dfm;
  }

  /* --- Nets ---------------------------------------------------------------------------------------------------- */

  async surlignerNet(nom: string) {
    const doc = this.hote.doc();
    if (!doc) return;
    if (this.net) for (const g of this.net.couches.values()) this.hote.rendu().liberer(g);
    this.net = null;
    const fait = await this.hote.attendre(L(`Recherche du net « ${nom} »…`, `Looking up net “${nom}”…`), async () => {
      const info = await moteur.netInfo(doc.lot, nom);
      const couches = new Map<number, CoucheGpu>();
      for (const c of info.couches) {
        if (c.genre === "percage" || c.genre === "fraisage" || c.genre === "composants") continue;
        couches.set(c.id, this.hote.rendu().charger(await moteur.netGeometrie(doc.lot, c.id, nom)));
      }
      return { info, couches };
    });
    if (!fait) return;
    this.net = { nom, ...fait };
    this.hote.montrerOnglet("net");
    this.hote.message(L(`Net « ${nom} » : ${longueur(fait.info.longueur, this.hote.unite(), 2)} de pistes.`, `Net “${nom}”: ${longueur(fait.info.longueur, this.hote.unite(), 2)} of tracks.`), "succes");
    this.hote.redessiner();
  }

  effacerNet() {
    if (!this.net) return;
    for (const g of this.net.couches.values()) this.hote.rendu().liberer(g);
    this.net = null;
    this.hote.redessiner();
  }

  /** Couches à afficher : les autres estompées, le net par-dessus en blanc lumineux. */
  appliquerNet(couches: CoucheAffichee[], ids: number[]): CoucheAffichee[] {
    if (!this.net) return couches;
    const estompees = couches.map((c) => ({ ...c, opacite: c.opacite * 0.28 }));
    const net: CoucheAffichee[] = [];
    ids.forEach((id) => {
      const g = this.net!.couches.get(id);
      if (g) net.push({ gpu: g, couleur: [1, 0.97, 0.75], opacite: 1 });
    });
    return [...estompees, ...net];
  }

  ongletNet(): HTMLElement {
    const n = this.net;
    if (!n) return el("p", { class: "discret", text: L("Aucun net surligné.", "No net highlighted.") });
    const u = this.hote.unite();
    const cuivres = n.info.couches.filter((c) => c.genre === "cuivre");
    const table = el("table", { class: "tableau-net" },
      el("thead", {}, el("tr", {}, ...[L("Couche", "Layer"), L("Longueur", "Length"), L("Segments", "Segments"), L("Pastilles", "Pads"), L("Surfaces", "Areas")].map((x, i) => el("th", { class: i ? "nombre" : "", text: x })))),
      el("tbody", {}, ...cuivres.map((c) => el("tr", {}, el("td", { text: tm(c.nom) }), el("td", { class: "nombre", text: longueur(c.longueur, u, 2) }), el("td", { class: "nombre", text: nombre(c.traits, 0) }), el("td", { class: "nombre", text: nombre(c.pastilles, 0) }), el("td", { class: "nombre", text: nombre(c.surfaces, 0) })))));
    return el("div", {},
      el("section", { class: "bloc" }, el("h3", { text: L("Net surligné", "Highlighted net") }),
        el("p", { class: "nom-net", text: n.nom }),
        el("dl", { class: "definitions" },
          el("dt", { text: L("Longueur des pistes", "Track length") }), el("dd", { text: longueur(n.info.longueur, u, 3) }),
          el("dt", { text: L("Couches de cuivre", "Copper layers") }), el("dd", { text: String(cuivres.length) }),
          el("dt", { text: L("Trous (vias, broches)", "Holes (vias, pins)") }), el("dd", { text: nombre(n.info.trous, 0) })),
        el("div", { class: "actions" },
          n.info.boite ? el("button", { type: "button", class: "bouton", html: `${icone("ajuster", 16)}<span>${L("Cadrer le net", "Fit the net")}</span>`, onclick: () => this.hote.ajuster(n.info.boite!) }) : null,
          el("button", { type: "button", class: "bouton", html: `${icone("fermer", 16)}<span>${L("Effacer", "Clear")}</span>`, onclick: () => {
            this.effacerNet();
            this.hote.montrerOnglet("inspecteur");
          } }))),
      el("section", { class: "bloc" }, el("h3", { text: L("Par couche", "Per layer") }), table,
        el("p", { class: "discret", text: L("Longueur mesurée sur l'axe des pistes, sans les plans ni la traversée des vias.", "Length measured along track centrelines, excluding planes and via barrels.") })));
  }

  /* --- DFM ----------------------------------------------------------------------------------------------------- */

  async verifier() {
    const doc = this.hote.doc();
    if (!doc) return;
    const r = await this.hote.attendre(L("Vérification DFM en cours…", "Running DFM checks…"), () => moteur.dfm(doc.lot, this.seuils));
    if (!r) return;
    this.dfm = { resultat: r, regle: null, choisi: -1 };
    const total = r.resultats.length;
    this.hote.message(total ? L(`DFM : ${total} point(s) à examiner (${nombre(r.duree / 1000, 1)} s).`, `DFM: ${total} issue(s) to review (${nombre(r.duree / 1000, 1)} s).`) : L("DFM : aucun défaut trouvé.", "DFM: no issue found."), total ? "info" : "succes");
    this.hote.montrerOnglet("dfm");
    this.hote.redessiner();
  }

  ongletDfm(): HTMLElement {
    const racine = el("div", { class: "dfm" });
    const u = this.hote.unite();
    // Profil et seuils
    const profil = el("select", { "aria-label": L("Profil du fabricant", "Manufacturer profile"), onchange: (e: Event) => {
      const p = PROFILS.find((x) => x.id === (e.target as HTMLSelectElement).value);
      if (p) {
        this.profil = p.id;
        this.seuils = { ...p.seuils };
        this.memoriser();
        this.hote.montrerOnglet("dfm");
      }
    } }, ...PROFILS.map((p) => el("option", { value: p.id, text: p.nom, selected: p.id === this.profil })), el("option", { value: "perso", text: L("Personnalisé", "Custom"), selected: this.profil === "perso" }));
    const champs = el("div", { class: "seuils" }, ...SEUILS.map((k) => el("label", { class: "seuil" }, el("span", { text: nomRegle(k === "serigraphie" ? "serigraphie" : k) }),
      el("input", { type: "number", min: "0", step: "0.01", value: String(this.seuils[k]), onchange: (e: Event) => {
        const v = Number((e.target as HTMLInputElement).value);
        if (Number.isFinite(v) && v >= 0) {
          this.seuils[k] = v;
          this.profil = "perso";
          this.memoriser();
        }
      } }), el("span", { class: "unite", text: "mm" }))));
    racine.append(el("section", { class: "bloc" }, el("h3", { text: L("Règles de fabrication", "Manufacturing rules") }),
      el("label", { class: "champ" }, el("span", { text: L("Profil", "Profile") }), profil),
      el("details", { class: "details-seuils" }, el("summary", { text: L("Seuils (mm)", "Thresholds (mm)") }), champs),
      el("p", { class: "discret", text: L("Valeurs indicatives : vérifiez les capacités exactes auprès de votre fabricant.", "Indicative values: check your manufacturer's exact capabilities.") }),
      el("div", { class: "actions" },
        el("button", { type: "button", class: "bouton principal", html: `${icone("zoom", 16)}<span>${L("Lancer la vérification", "Run the checks")}</span>`, onclick: () => void this.verifier() }),
        this.dfm ? el("button", { type: "button", class: "bouton", html: `${icone("pdf", 16)}<span>${L("Rapport PDF", "PDF report")}</span>`, onclick: () => void this.rapport() }) : null)));
    const d = this.dfm;
    if (!d) {
      racine.append(el("p", { class: "discret", text: L("La vérification porte sur : largeur des pistes, isolement et courts-circuits entre nets, anneau annulaire et pastilles des trous métallisés, perçages et écarts entre trous, cuivre au bord de la carte et près des trous non métallisés, ponts de vernis, sérigraphie (traits fins, sur les pastilles, hors de la carte).", "Checks: track width, clearance and shorts between nets, annular ring and pads of plated holes, drill sizes and hole spacing, copper to board edge and to non-plated holes, solder mask bridges, silkscreen (thin lines, on pads, off board).") }));
      return racine;
    }
    // Bilan par règle
    const r = d.resultat;
    const liste = el("ul", { class: "regles-dfm" });
    for (const regle of ORDRE) {
      const verifiee = r.verifiees.includes(regle) || (regle === "pastilleAbsente" && r.verifiees.includes("anneau"));
      const n = r.comptes[regle] ?? 0;
      const g = REGLES[regle].gravite;
      liste.append(el("li", {}, el("button", { type: "button", class: `regle ${d.regle === regle ? "actif" : ""}`, disabled: !n, onclick: () => {
        d.regle = d.regle === regle ? null : regle;
        d.choisi = -1;
        this.hote.montrerOnglet("dfm");
        this.hote.redessiner();
      } }, el("span", { class: `etat-regle ${n ? g : verifiee ? "ok" : "non"}`, text: n ? nombre(n, 0) : verifiee ? "✓" : "–" }), el("span", { text: nomRegle(regle) }),
        el("span", { class: "seuil-regle", text: REGLES[regle].seuil ? `≥ ${longueur(this.seuils[REGLES[regle].seuil!], u, 3)}` : !verifiee ? L("non vérifiée", "not checked") : "" }))));
    }
    const total = r.resultats.length;
    racine.append(el("section", { class: "bloc" }, el("h3", { text: L("Résultats", "Results") }),
      el("p", { class: total ? "" : "succes-dfm", text: total ? L(`${total} point(s) à examiner, en ${nombre(r.duree / 1000, 1)} s. Cliquez sur une règle pour voir ses défauts sur la carte.`, `${total} issue(s) to review, in ${nombre(r.duree / 1000, 1)} s. Click a rule to see its issues on the board.`) : L("Aucun défaut trouvé avec ces seuils.", "No issue found with these thresholds.") }),
      liste,
      r.contour ? null : el("p", { class: "discret", text: L("Pas de contour de carte : le cuivre au bord et la sérigraphie hors carte ne sont pas vérifiés.", "No board outline: copper-to-edge and off-board silkscreen are not checked.") })));
    // Défauts de la règle choisie
    if (d.regle) {
      const defauts = r.resultats.filter((x) => x.regle === d.regle);
      const ul = el("ul", { class: "liste-composants defauts-dfm" });
      defauts.slice(0, 500).forEach((x, i) => ul.append(el("li", {}, el("button", { type: "button", class: i === d.choisi ? "actif" : "", onclick: () => {
        d.choisi = i;
        this.hote.allerA(x.x, x.y, 2.5);
        this.hote.montrerOnglet("dfm");
      } }, el("strong", { text: `${i + 1}.` }), el("span", { text: this.texteDefaut(x) })))));
      if (defauts.length > 500) ul.append(el("li", { class: "discret", text: L(`… et ${defauts.length - 500} autres`, `… and ${defauts.length - 500} more`) }));
      racine.append(el("section", { class: "bloc" }, el("h3", { text: nomRegle(d.regle) }), ul));
    }
    return racine;
  }

  private texteDefaut(x: DefautDfm): string {
    const u = this.hote.unite();
    const lieu = `${tm(x.couche) ? `${tm(x.couche)} · ` : ""}X ${longueur(x.x, u, 2)} Y ${longueur(x.y, u, 2)}`;
    if (x.regle === "court") return `${lieu} · ${x.details}`;
    if (x.regle === "pastilleAbsente") return `${lieu} · ${x.details}`;
    if (REGLES[x.regle]?.surface) return `${lieu} · ${nombre(x.valeur, 3)} mm²`;
    return `${lieu} · ${longueur(x.valeur, u, 3)} < ${longueur(x.limite, u, 3)}${x.details ? ` · ${tm(x.details)}` : ""}`;
  }

  /** Marques des défauts de la règle choisie (ou de toutes, en petit) sur la surcouche. */
  dessinerMarques(ctx: CanvasRenderingContext2D, versEcran: (x: number, y: number) => [number, number]) {
    const d = this.dfm;
    if (!d) return;
    const r = window.devicePixelRatio || 1;
    const defauts = d.regle ? d.resultat.resultats.filter((x) => x.regle === d.regle) : d.resultat.resultats;
    ctx.save();
    defauts.slice(0, 3000).forEach((x, i) => {
      const [sx, sy] = versEcran(x.x, x.y);
      const g = REGLES[x.regle]?.gravite ?? "info";
      const choisi = d.regle && i === d.choisi;
      const rayon = (choisi ? 16 : d.regle ? 9 : 5) * r;
      ctx.strokeStyle = COULEURS[g];
      ctx.lineWidth = (choisi ? 2.5 : 1.5) * r;
      ctx.beginPath();
      ctx.arc(sx, sy, rayon, 0, Math.PI * 2);
      ctx.stroke();
      if (choisi) {
        ctx.beginPath();
        ctx.moveTo(sx - rayon * 1.6, sy);
        ctx.lineTo(sx - rayon * 0.6, sy);
        ctx.moveTo(sx + rayon * 0.6, sy);
        ctx.lineTo(sx + rayon * 1.6, sy);
        ctx.moveTo(sx, sy - rayon * 1.6);
        ctx.lineTo(sx, sy - rayon * 0.6);
        ctx.moveTo(sx, sy + rayon * 0.6);
        ctx.lineTo(sx, sy + rayon * 1.6);
        ctx.stroke();
      }
    });
    ctx.restore();
  }

  /** Rapport PDF : bilan par règle, vues de dessus et de dessous avec les marques, puis la liste des défauts. */
  async rapport() {
    const doc = this.hote.doc();
    const d = this.dfm;
    if (!doc || !d) return;
    const fait = await this.hote.attendre(L("Préparation du rapport DFM…", "Preparing the DFM report…"), async () => {
      const { PDFDocument, rgb, StandardFonts } = await import("pdf-lib");
      const { vue2d } = await import("./fiche");
      const pdf = await PDFDocument.create();
      pdf.setTitle(`DFM ${doc.nom}`);
      pdf.setCreator("Typon");
      const police = await pdf.embedFont(StandardFonts.Helvetica);
      const gras = await pdf.embedFont(StandardFonts.HelveticaBold);
      const propre = (s: string) => String(s).replace(/[  ]/g, " ").replace(/≥/g, ">=").replace(/≤/g, "<=").replace(/✓/g, "OK").replace(/[^\x20-\x7e -ÿ€–—…’‘“”•×]/g, "?");
      const P = { l: 595.28, h: 841.89 };
      const M = 40;
      const u = this.hote.unite();
      const couleur = (g: Gravite) => (g === "erreur" ? rgb(0.85, 0.2, 0.2) : g === "avertissement" ? rgb(0.9, 0.5, 0.1) : rgb(0.75, 0.6, 0.1));
      const date = new Date().toLocaleString(locale(), { dateStyle: "long", timeStyle: "short" });
      let page = pdf.addPage([P.l, P.h]);
      const ecrire = (t: string, x: number, y: number, taille = 9, f = police, c = rgb(0.08, 0.1, 0.13)) => page.drawText(propre(t), { x, y, size: taille, font: f, color: c });
      const entete = (titre: string) => {
        page.drawRectangle({ x: 0, y: P.h - 6, width: P.l, height: 6, color: rgb(0.91, 0.45, 0.23) });
        ecrire(titre, M, P.h - M - 6, 15, gras);
        ecrire(doc.nom, P.l - M - police.widthOfTextAtSize(propre(doc.nom), 8), P.h - M - 4, 8, police, rgb(0.4, 0.43, 0.48));
      };
      entete(L("Rapport DFM", "DFM report"));
      let y = P.h - M - 30;
      ecrire(L(`Établi par Typon le ${date} — profil : ${PROFILS.find((p) => p.id === this.profil)?.nom ?? L("personnalisé", "custom")}`, `Generated by Typon on ${date} — profile: ${PROFILS.find((p) => p.id === this.profil)?.nom ?? "custom"}`), M, y, 8, police, rgb(0.4, 0.43, 0.48));
      y -= 26;
      ecrire(L("Bilan", "Summary"), M, y, 12, gras);
      y -= 18;
      for (const regle of ORDRE) {
        const n = d.resultat.comptes[regle] ?? 0;
        const verifiee = d.resultat.verifiees.includes(regle) || (regle === "pastilleAbsente" && d.resultat.verifiees.includes("anneau"));
        const g = REGLES[regle].gravite;
        ecrire(nomRegle(regle), M, y, 9.5, gras);
        const s = REGLES[regle].seuil;
        ecrire(s ? `>= ${longueur(this.seuils[s], u, 3)}` : "", M + 230, y, 9, police, rgb(0.4, 0.43, 0.48));
        ecrire(n ? L(`${n} point(s)`, `${n} issue(s)`) : verifiee ? L("conforme", "pass") : L("non vérifiée", "not checked"), M + 330, y, 9.5, gras, n ? couleur(g) : verifiee ? rgb(0.13, 0.55, 0.3) : rgb(0.4, 0.43, 0.48));
        y -= 16;
      }
      y -= 10;
      ecrire(L("Les seuils sont ceux choisis dans Typon ; un défaut signalé est à confirmer dans le logiciel de conception.", "Thresholds as chosen in Typon; each reported issue should be confirmed in the design software."), M, y, 8, police, rgb(0.4, 0.43, 0.48));
      // Vues avec marques
      const b = doc.boite;
      if (b) {
        for (const cote of ["dessus", "dessous"] as const) {
          page = pdf.addPage([P.l, P.h]);
          entete(cote === "dessus" ? L("Vue de dessus", "Top view") : L("Vue de dessous (miroir)", "Bottom view (mirrored)"));
          const m = Math.max(b[2] - b[0], b[3] - b[1]) * 0.03;
          const cadre: [number, number, number, number] = [b[0] - m, b[1] - m, b[2] + m, b[3] + m];
          const zone = { w: P.l - 2 * M, h: P.h - 2 * M - 60 };
          const e = Math.min(zone.w / (cadre[2] - cadre[0]), zone.h / (cadre[3] - cadre[1]));
          const w = (cadre[2] - cadre[0]) * e;
          const h = (cadre[3] - cadre[1]) * e;
          const x0 = M + (zone.w - w) / 2;
          const y0 = P.h - M - 40 - h;
          const png = await vue2d({ doc, rendu: this.hote.rendu(), unite: u }, cote, cadre, Math.round(w * 3), Math.round(h * 3));
          page.drawImage(await pdf.embedPng(png), { x: x0, y: y0, width: w, height: h });
          const miroir = cote === "dessous";
          d.resultat.resultats.slice(0, 3000).forEach((x) => {
            const g = REGLES[x.regle]?.gravite ?? "info";
            const px = miroir ? x0 + (cadre[2] - x.x) * e : x0 + (x.x - cadre[0]) * e;
            const py = y0 + (x.y - cadre[1]) * e;
            page.drawCircle({ x: px, y: py, size: 4, borderColor: couleur(g), borderWidth: 1 });
          });
          ecrire(L("Cercles : rouge = erreur, orange = avertissement, jaune = remarque. Tous les défauts sont marqués sur les deux vues.", "Circles: red = error, orange = warning, yellow = note. All issues are marked on both views."), M, y0 - 16, 8, police, rgb(0.4, 0.43, 0.48));
        }
      }
      // Liste
      page = pdf.addPage([P.l, P.h]);
      entete(L("Défauts", "Issues"));
      y = P.h - M - 34;
      for (const regle of ORDRE) {
        const defauts = d.resultat.resultats.filter((x) => x.regle === regle);
        if (!defauts.length) continue;
        if (y < M + 60) {
          page = pdf.addPage([P.l, P.h]);
          entete(L("Défauts (suite)", "Issues (continued)"));
          y = P.h - M - 34;
        }
        ecrire(`${nomRegle(regle)} (${defauts.length})`, M, y, 11, gras, couleur(REGLES[regle].gravite));
        y -= 15;
        for (const [i, x] of defauts.slice(0, 300).entries()) {
          if (y < M + 10) {
            page = pdf.addPage([P.l, P.h]);
            entete(L("Défauts (suite)", "Issues (continued)"));
            y = P.h - M - 34;
          }
          let texte = `${i + 1}. ${this.texteDefaut(x)}`;
          while (texte.length > 4 && police.widthOfTextAtSize(propre(texte), 8) > P.l - 2 * M) texte = `${texte.slice(0, -2)}…`;
          ecrire(texte, M + 8, y, 8);
          y -= 11.5;
        }
        if (defauts.length > 300) {
          ecrire(L(`… et ${defauts.length - 300} autres`, `… and ${defauts.length - 300} more`), M + 8, y, 8, police, rgb(0.4, 0.43, 0.48));
          y -= 11.5;
        }
        y -= 8;
      }
      pdf.getPages().forEach((p, i, t) => p.drawText(`${i + 1} / ${t.length}`, { x: P.l - M - 24, y: 20, size: 8, font: police, color: rgb(0.4, 0.43, 0.48) }));
      telecharger((await pdf.save()) as BlobPart, `DFM ${doc.nom}.pdf`.replace(/[\\/:*?"<>|]/g, "_"), "application/pdf");
      return true;
    });
    this.hote.redessiner();
    if (fait) this.hote.message(L("Rapport DFM enregistré (dossier des téléchargements).", "DFM report saved (downloads folder)."), "succes");
  }

  /* --- Panneau ------------------------------------------------------------------------------------------------- */

  dialoguePanneau() {
    const doc = this.hote.doc();
    if (!doc) return;
    const b = doc.boite;
    const carte = b ? [b[2] - b[0], b[3] - b[1]] : [0, 0];
    const p = { nx: 2, ny: 2, separation: "fraisage", ecartX: 2, ecartY: 2, bord: 5, bords: "haut-bas", mires: true, trousOutil: true };
    try {
      Object.assign(p, JSON.parse(localStorage.getItem("typon-panneau") ?? "{}"));
    } catch {
      /* stockage indisponible */
    }
    const taille = el("p", { class: "taille-panneau" });
    const majTaille = () => {
      const bh = p.bords === "tous" || p.bords === "haut-bas" ? p.bord : 0;
      const bv = p.bords === "tous" || p.bords === "gauche-droite" ? p.bord : 0;
      const l = 2 * bv + p.nx * carte[0] + (p.nx - 1) * p.ecartX;
      const h = 2 * bh + p.ny * carte[1] + (p.ny - 1) * p.ecartY;
      taille.textContent = L(`Panneau de ${p.nx * p.ny} cartes : ${longueur(l, this.hote.unite(), 1)} × ${longueur(h, this.hote.unite(), 1)}`, `Panel of ${p.nx * p.ny} boards: ${longueur(l, this.hote.unite(), 1)} × ${longueur(h, this.hote.unite(), 1)}`);
    };
    const nombreChamp = (cle: "nx" | "ny" | "ecartX" | "ecartY" | "bord", libelle: string, min: number, max: number, pas: number) =>
      el("label", { class: "champ" }, el("span", { text: libelle }), el("input", { type: "number", min: String(min), max: String(max), step: String(pas), value: String(p[cle]), oninput: (e: Event) => {
        const v = Number((e.target as HTMLInputElement).value);
        if (Number.isFinite(v)) p[cle] = Math.min(max, Math.max(min, v));
        majTaille();
      } }));
    const choix = (cle: "separation" | "bords", libelle: string, options: [string, string][]) =>
      el("label", { class: "champ" }, el("span", { text: libelle }), el("select", { onchange: (e: Event) => {
        p[cle] = (e.target as HTMLSelectElement).value;
        if (cle === "separation") {
          // Rainures en V : les cartes se touchent ; fraisage : 2 mm pour la fraise.
          p.ecartX = p.ecartY = p.separation === "vscore" ? 0 : 2;
          formulaire.querySelectorAll<HTMLInputElement>("input[data-ecart]").forEach((i) => (i.value = String(p.ecartX)));
        }
        majTaille();
      } }, ...options.map(([v, t]) => el("option", { value: v, text: t, selected: p[cle] === v }))));
    const caseACocher = (cle: "mires" | "trousOutil", libelle: string) =>
      el("label", { class: "case" }, el("input", { type: "checkbox", checked: p[cle], onchange: (e: Event) => (p[cle] = (e.target as HTMLInputElement).checked) }), el("span", { text: libelle }));
    const ecartX = nombreChamp("ecartX", L("Écart horizontal (mm)", "Horizontal gap (mm)"), 0, 50, 0.5);
    const ecartY = nombreChamp("ecartY", L("Écart vertical (mm)", "Vertical gap (mm)"), 0, 50, 0.5);
    ecartX.querySelector("input")!.dataset.ecart = "x";
    ecartY.querySelector("input")!.dataset.ecart = "y";
    const formulaire = el("div", { class: "grille-panneau" },
      nombreChamp("nx", L("Colonnes", "Columns"), 1, 30, 1),
      nombreChamp("ny", L("Lignes", "Rows"), 1, 30, 1),
      choix("separation", L("Séparation", "Separation"), [["fraisage", L("Fraisage et languettes perforées", "Routing with mouse-bite tabs")], ["vscore", L("Rainures en V", "V-score")]]),
      ecartX, ecartY,
      nombreChamp("bord", L("Bords de manutention (mm)", "Handling rails (mm)"), 0, 50, 0.5),
      choix("bords", L("Bords", "Rails"), [["haut-bas", L("Haut et bas", "Top and bottom")], ["gauche-droite", L("Gauche et droite", "Left and right")], ["tous", L("Les quatre", "All four")], ["aucun", L("Aucun", "None")]]),
      caseACocher("mires", L("Mires optiques sur les bords", "Fiducials on the rails")),
      caseACocher("trousOutil", L("Trous d'outillage (Ø 3 mm) aux coins", "Tooling holes (Ø 3 mm) in the corners")));
    majTaille();
    const dialogue = el("dialog", { class: "dialogue" });
    const fabriquer = async () => {
      try {
        localStorage.setItem("typon-panneau", JSON.stringify(p));
      } catch {
        /* stockage indisponible */
      }
      const r = await this.hote.attendre(L("Construction du panneau…", "Building the panel…"), () => moteur.panneau(doc.lot, p));
      if (!r) return null;
      if (r.panneau.erreur) {
        this.hote.message(tm(r.panneau.erreur), "erreur");
        return null;
      }
      return r;
    };
    const nomZip = `panneau ${p.nx}x${p.ny} ${doc.nom}`.replace(/[\\/:*?"<>|]/g, "_");
    dialogue.append(
      el("h2", { html: `${icone("couches", 18)} ${L("Mise en panneau", "Panelization")}` }),
      el("p", { class: "discret", text: L(`Carte de ${longueur(carte[0], this.hote.unite(), 2)} × ${longueur(carte[1], this.hote.unite(), 2)}. Le panneau est écrit en Gerber X2 et Excellon (archive ZIP), quel que soit le format d'origine.`, `Board of ${longueur(carte[0], this.hote.unite(), 2)} × ${longueur(carte[1], this.hote.unite(), 2)}. The panel is written as Gerber X2 and Excellon (ZIP archive), whatever the source format.`) }),
      el("p", { class: "avertissement-panneau", html: `${icone("alerte", 15)} ${L("<strong>Aperçu</strong> : ne pas envoyer ces fichiers en fabrication pour l'instant. Le détourage des cartes, les perçages borgnes ou enterrés et la place des mires sont encore à corriger.", "<strong>Preview</strong>: do not send these files to manufacturing yet. Board routing, blind or buried drills and fiducial placement still need fixing.")}` }),
      formulaire, taille,
      el("div", { class: "actions" },
        el("button", { type: "button", class: "bouton", text: L("Fermer", "Close"), onclick: () => dialogue.close() }),
        el("button", { type: "button", class: "bouton", html: `${icone("telecharger", 16)}<span>${L("Télécharger (ZIP)", "Download (ZIP)")}</span>`, onclick: async () => {
          const r = await fabriquer();
          if (!r) return;
          telecharger(r.zip as BlobPart, `${nomZip}.zip`, "application/zip");
          this.hote.message(L(`Panneau enregistré : ${r.panneau.fichiers?.length ?? 0} fichiers.`, `Panel saved: ${r.panneau.fichiers?.length ?? 0} files.`), "succes");
        } }),
        el("button", { type: "button", class: "bouton principal", html: `${icone("carte", 16)}<span>${L("Voir le panneau", "View the panel")}</span>`, onclick: async () => {
          const r = await fabriquer();
          if (!r) return;
          dialogue.close();
          await this.hote.ouvrirFichiers([new File([r.zip as BlobPart], `${nomZip}.zip`, { type: "application/zip" })], "A", nomZip);
          this.hote.message(L("Aperçu du panneau ouvert. Pour revenir à la carte, rouvrez ses fichiers.", "Panel preview opened. To go back to the board, reopen its files."), "succes");
        } })));
    dialogue.addEventListener("close", () => dialogue.remove());
    document.body.append(dialogue);
    dialogue.showModal();
  }
}
