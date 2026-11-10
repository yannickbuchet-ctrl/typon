/**
 * Application Typon : accueil, visionneuse, panneaux, comparaison.
 */
import type { ObjetTrouve } from "../moteur/client";
import { moteur } from "../moteur/client";
import { couleurRgb, type CoucheAffichee } from "../rendu/rendu";
import { alignementAuto, analyser, apparier, comparerComposants, type Paire, type ZoneMm } from "./comparaison";
import { DocumentTypon, ouvrir, type Cote, type EtatCouche } from "./document";
import { el, longueur, nombre, pluriel, type Unite } from "./dom";
import { icone, LOGO } from "./icones";
import { detailsCouche, onglets, resume, type Onglet } from "./panneaux";
import { Visionneuse, type Panneau, type Pointeur } from "./visionneuse";

type Outil = "info" | "mesure";
type Affichage = "superposition" | "cote" | "alterner";

const EXEMPLES = [
  { fichier: "exemples/typon-demo-revA.zip", titre: "Carte d'exemple", texte: "Gerber X2 et perçages d'une carte fictive à 4 couches", icone: "carte" },
  { fichier: "exemples/typon-demo-revA-odb.tgz", titre: "La même en ODB++", texte: "Composants, nets, nomenclature mis en forme", icone: "puce" },
  { fichier: "comparer", titre: "Comparer deux révisions", texte: "Révision A et révision B de la carte d'exemple", icone: "comparer" },
];

export class App {
  private racine: HTMLElement;
  private visionneuse: Visionneuse | null = null;
  private zoneVue: HTMLElement;
  private panneauGauche: HTMLElement;
  private panneauDroit: HTMLElement;
  private barre: HTMLElement;
  private etat: HTMLElement;
  private hud: HTMLElement;
  private docA: DocumentTypon | null = null;
  private docB: DocumentTypon | null = null;
  private cote: Cote = "dessus";
  private outil: Outil = "info";
  private unite: Unite = "mm";
  private selection: { doc: DocumentTypon; couche: EtatCouche; objet: ObjetTrouve; panneau: number } | null = null;
  private mesure: { a?: { x: number; y: number }; b?: { x: number; y: number }; panneau: number } = { panneau: 0 };
  private ongletActif = "carte";
  private comparaison: { paires: Paire[]; choisie: number; affichage: Affichage; montrerB: boolean; zone: number; decalage: { dx: number; dy: number }; aligner: boolean; tolerance: number; analyse: boolean } | null = null;
  private occupe = 0;

  constructor(racine: HTMLElement) {
    this.racine = racine;
    this.barre = el("header", { class: "barre" });
    this.panneauGauche = el("aside", { class: "panneau gauche", "aria-label": "Couches" });
    this.panneauDroit = el("aside", { class: "panneau droit", "aria-label": "Informations" });
    this.zoneVue = el("main", { class: "zone-vue" });
    this.hud = el("div", { class: "hud", "aria-live": "off" });
    this.etat = el("div", { class: "etat", role: "status", "aria-live": "polite" });
    racine.append(this.barre, el("div", { class: "corps" }, this.panneauGauche, this.zoneVue, this.panneauDroit), el("footer", { class: "pied" }, this.etat, el("span", { class: "confidentialite", html: `${icone("info", 14)} Vos fichiers restent sur cet appareil : rien n'est envoyé.` })));
    this.installerDepot();
    this.installerClavier();
    this.theme(localStorage.getItem("typon-theme") === "clair" ? "clair" : "sombre");
    this.afficher();
  }

  /* --- Messages et attente ---------------------------------------------------------------------------------------- */

  message(texte: string, genre: "info" | "erreur" | "succes" = "info") {
    this.etat.textContent = texte;
    this.etat.className = `etat ${genre}`;
  }

  private async attendre<T>(texte: string, travail: () => Promise<T>): Promise<T | undefined> {
    this.occupe++;
    this.racine.classList.add("occupe");
    this.message(texte);
    try {
      return await travail();
    } catch (e) {
      console.error(e);
      this.message((e as Error).message || String(e), "erreur");
      return undefined;
    } finally {
      this.occupe--;
      if (!this.occupe) this.racine.classList.remove("occupe");
    }
  }

  /* --- Ouverture des fichiers --------------------------------------------------------------------------------------- */

  private choisirFichiers(dossier = false): Promise<File[]> {
    return new Promise((ok) => {
      const champ = el("input", { type: "file", multiple: true, style: { display: "none" } });
      if (dossier) champ.setAttribute("webkitdirectory", "");
      let fini = false;
      const finir = (fichiers: File[]) => {
        if (fini) return;
        fini = true;
        ok(fichiers);
        champ.remove();
      };
      champ.addEventListener("change", () => finir([...(champ.files ?? [])]));
      // Firefox peut annoncer « cancel » juste avant « change » : on attend un instant avant de conclure à l'abandon.
      champ.addEventListener("cancel", () => setTimeout(() => finir([...(champ.files ?? [])]), 400));
      document.body.append(champ);
      champ.click();
    });
  }

  /** Ouvre un jeu de fichiers : en A (visionneuse), ou en B (comparaison). */
  async ouvrirFichiers(fichiers: File[], role: "A" | "B" = "A", nom?: string) {
    if (!fichiers.length) return;
    const doc = await this.attendre(`Lecture de ${fichiers.length > 1 ? pluriel(fichiers.length, "fichier") : `« ${fichiers[0].name} »`}…`, () => ouvrir(fichiers, nom));
    if (!doc) return;
    if (!doc.couches.length) {
      doc.fermer(this.vue().rendu);
      const ignores = doc.description.ignores;
      this.message(`Aucune couche Gerber, perçage ou ODB++ reconnue${ignores.length ? ` (${ignores.slice(0, 5).join(", ")}${ignores.length > 5 ? "…" : ""})` : ""}.${doc.description.alertes.length ? ` ${doc.description.alertes[0]}` : ""}`, "erreur");
      return;
    }
    if (role === "A") {
      this.docA?.fermer(this.vue().rendu);
      this.docA = doc;
      if (this.comparaison && this.docB) this.demarrerComparaison();
      else {
        this.docB?.fermer(this.vue().rendu);
        this.docB = null;
        this.comparaison = null;
      }
    } else {
      this.docB?.fermer(this.vue().rendu);
      this.docB = doc;
      if (this.docA) this.demarrerComparaison();
    }
    this.selection = null;
    this.mesure = { panneau: 0 };
    this.afficher();
    this.vue().ajuster(this.docA?.boite ?? doc.boite);
    const alertes = doc.description.alertes.length + (doc.description.meta.odb?.alertes.length ?? 0) + doc.couches.reduce((s, c) => s + (c.desc.meta.alertes?.length ?? 0), 0);
    this.message(`« ${doc.nom} » : ${pluriel(doc.couches.length, "couche")} lue${doc.couches.length > 1 ? "s" : ""} en ${nombre(doc.duree / 1000, 2)} s.${alertes ? ` ${pluriel(alertes, "remarque")} dans le panneau Carte.` : ""}`, "succes");
  }

  async ouvrirExemple(fichier: string, role: "A" | "B" = "A") {
    const reponse = await this.attendre("Chargement de l'exemple…", () => fetch(fichier).then((r) => {
      if (!r.ok) throw new Error(`Exemple introuvable (${r.status}).`);
      return r.blob();
    }));
    if (!reponse) return;
    const nom = fichier.split("/").pop()!;
    await this.ouvrirFichiers([new File([reponse], nom)], role, nom.replace(/\.(zip|tgz)$/, ""));
  }

  private installerDepot() {
    let compteur = 0;
    const cible = () => document.body;
    window.addEventListener("dragenter", (e) => {
      if (!e.dataTransfer?.types.includes("Files")) return;
      compteur++;
      cible().classList.add("depot-actif");
    });
    window.addEventListener("dragleave", () => {
      if (--compteur <= 0) {
        compteur = 0;
        cible().classList.remove("depot-actif");
      }
    });
    window.addEventListener("dragover", (e) => e.preventDefault());
    window.addEventListener("drop", async (e) => {
      e.preventDefault();
      compteur = 0;
      cible().classList.remove("depot-actif");
      const fichiers = await fichiersDeposes(e.dataTransfer);
      // Déposé sur la moitié droite en comparaison, ou sur la case « B » : version B.
      const versB = (e.target as HTMLElement | null)?.closest?.("[data-depot='B']") !== null;
      await this.ouvrirFichiers(fichiers, versB && this.docA ? "B" : "A");
    });
  }

  private installerClavier() {
    window.addEventListener("keydown", (e) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement || e.ctrlKey || e.metaKey || e.altKey) return;
      if (!this.docA) return;
      const k = e.key.toLowerCase();
      if (k === "f") this.vue().ajuster(this.docA.boite);
      else if (k === "b") this.retourner();
      else if (k === "m") this.choisirOutil(this.outil === "mesure" ? "info" : "mesure");
      else if (k === "escape") {
        this.selection = null;
        this.mesure = { panneau: 0 };
        this.vue().redessiner();
        this.afficherPanneauDroit();
      } else if (this.comparaison && (k === "n" || k === "p")) this.allerZone(this.comparaison.zone + (k === "n" ? 1 : -1));
      else if (this.comparaison && k === " ") {
        e.preventDefault();
        this.comparaison.montrerB = !this.comparaison.montrerB;
        this.afficherBarre();
        this.vue().redessiner();
      } else if (this.comparaison && ["1", "2", "3"].includes(k)) this.choisirAffichage((["superposition", "cote", "alterner"] as Affichage[])[Number(k) - 1]);
      else return;
    });
  }

  /* --- Mise en page --------------------------------------------------------------------------------------------------- */

  private vue(): Visionneuse {
    if (!this.visionneuse) {
      const v = new Visionneuse();
      v.panneaux = () => this.panneaux();
      v.dessinerSurcouche = (ctx, versEcran) => this.dessinerSurcouche(ctx, versEcran);
      v.surDeplacement = (p) => this.deplacement(p);
      v.surClic = (p) => void this.clic(p);
      v.surChangementVue = () => this.majHud();
      this.visionneuse = v;
    }
    return this.visionneuse;
  }

  private afficher() {
    this.afficherBarre();
    this.racine.classList.toggle("avec-document", !!this.docA);
    this.racine.classList.toggle("en-comparaison", !!this.comparaison);
    if (!this.docA) {
      this.panneauGauche.replaceChildren();
      this.panneauDroit.replaceChildren();
      this.zoneVue.replaceChildren(this.accueil());
      return;
    }
    if (!this.zoneVue.contains(this.vue().racine)) this.zoneVue.replaceChildren(this.vue().racine, this.hud);
    this.afficherPanneauGauche();
    this.afficherPanneauDroit();
    this.chargerVisibles();
    this.vue().redessiner();
  }

  private afficherBarre() {
    const b = (ic: string, titre: string, action: () => void, opts: { texte?: string; actif?: boolean; classe?: string } = {}) => {
      const bt = el("button", { type: "button", class: `bouton ${opts.classe ?? ""} ${opts.actif ? "actif" : ""}`, title: titre, "aria-label": titre, "aria-pressed": opts.actif === undefined ? undefined : String(opts.actif), onclick: action, html: `${icone(ic)}${opts.texte ? `<span>${opts.texte}</span>` : ""}` });
      return bt;
    };
    const elements: (HTMLElement | null)[] = [
      el("a", { class: "marque", href: "./", html: `${LOGO}<span>Typon</span>`, title: "Typon — accueil", onclick: (e: Event) => {
        e.preventDefault();
        this.fermerTout();
      } }),
    ];
    if (this.docA) {
      if (this.comparaison) {
        elements.push(el("div", { class: "pastilles-docs" },
          this.pastille("A", this.docA),
          b("echange", "Échanger A et B", () => this.echanger(), { classe: "discret" }),
          this.docB ? this.pastille("B", this.docB) : el("button", { type: "button", class: "bouton principal", "data-depot": "B", html: `${icone("ouvrir")}<span>Version B…</span>`, onclick: async () => this.ouvrirFichiers(await this.choisirFichiers(), "B") })));
        const segment = el("div", { class: "segment", role: "group", "aria-label": "Affichage de la comparaison" });
        for (const [m, t, ic] of [["superposition", "Superposition", "couches"], ["cote", "Côte à côte", "comparer"], ["alterner", "Alterner A/B", "echange"]] as const) {
          segment.append(b(ic, `${t} (${["superposition", "cote", "alterner"].indexOf(m) + 1})`, () => this.choisirAffichage(m), { texte: t, actif: this.comparaison.affichage === m }));
        }
        elements.push(segment);
        if (this.comparaison.affichage === "alterner") elements.push(b("echange", "Voir l'autre version (barre d'espace)", () => {
          this.comparaison!.montrerB = !this.comparaison!.montrerB;
          this.afficherBarre();
          this.vue().redessiner();
        }, { texte: this.comparaison.montrerB ? "B affichée" : "A affichée", classe: "bascule" }));
      } else {
        elements.push(el("div", { class: "pastilles-docs" }, this.pastille("", this.docA)));
        elements.push(b("ouvrir", "Ouvrir d'autres fichiers", async () => this.ouvrirFichiers(await this.choisirFichiers()), { texte: "Ouvrir" }));
        elements.push(b("comparer", "Comparer avec une autre version", async () => {
          this.comparaison = { paires: [], choisie: 0, affichage: "superposition", montrerB: false, zone: -1, decalage: { dx: 0, dy: 0 }, aligner: true, tolerance: 0.05, analyse: false };
          this.afficher();
          this.message("Choisissez ou déposez la version B (nouvelle version) à comparer.");
          const f = await this.choisirFichiers();
          if (f.length) await this.ouvrirFichiers(f, "B");
        }, { texte: "Comparer" }));
      }
      elements.push(el("span", { class: "espace" }));
      const seg = el("div", { class: "segment", role: "group", "aria-label": "Côté vu" });
      seg.append(b("carte", "Vue de dessus (B pour retourner)", () => this.choisirCote("dessus"), { texte: "Dessus", actif: this.cote === "dessus" }), b("retourner", "Vue de dessous, en miroir (B pour retourner)", () => this.choisirCote("dessous"), { texte: "Dessous", actif: this.cote === "dessous" }));
      elements.push(seg);
      const outils = el("div", { class: "segment", role: "group", "aria-label": "Outil" });
      outils.append(b("info", "Informations sur l'objet cliqué", () => this.choisirOutil("info"), { actif: this.outil === "info" }), b("mesure", "Mesurer une distance (M)", () => this.choisirOutil("mesure"), { actif: this.outil === "mesure" }));
      elements.push(outils);
      elements.push(b("ajuster", "Ajuster à la carte (F)", () => this.vue().ajuster(this.docA!.boite)));
      const unites = el("select", { class: "unites", "aria-label": "Unité", title: "Unité des mesures", onchange: (e: Event) => {
        this.unite = (e.target as HTMLSelectElement).value as Unite;
        this.majHud();
        this.afficherPanneauDroit();
        this.vue().redessiner();
      } }, ...(["mm", "mil", "pouce"] as Unite[]).map((u) => el("option", { value: u, text: u === "pouce" ? "pouces" : u, selected: u === this.unite })));
      elements.push(unites);
    } else {
      elements.push(el("span", { class: "espace" }));
    }
    elements.push(b(this.racine.dataset.theme === "clair" ? "lune" : "soleil", "Thème clair ou sombre", () => this.theme(this.racine.dataset.theme === "clair" ? "sombre" : "clair"), { classe: "discret" }));
    this.barre.replaceChildren(...elements.filter((x): x is HTMLElement => !!x));
  }

  private pastille(lettre: string, doc: DocumentTypon) {
    return el("span", { class: `pastille-doc ${lettre ? `doc-${lettre.toLowerCase()}` : ""}`, title: `${doc.nom} — ${pluriel(doc.couches.length, "couche")}` }, lettre ? el("b", { text: lettre }) : null, el("span", { text: doc.nom }));
  }

  private theme(t: "clair" | "sombre") {
    this.racine.dataset.theme = t;
    document.documentElement.dataset.theme = t;
    try {
      localStorage.setItem("typon-theme", t);
    } catch {
      /* stockage indisponible */
    }
    // La carte garde un fond sombre dans les deux thèmes (la sérigraphie est blanche) : seul l'habillage change.
    this.visionneuse?.redessiner();
    this.afficherBarre();
  }

  private accueil(): HTMLElement {
    const depot = el("div", { class: "depot", tabindex: "0", role: "button", "aria-label": "Ouvrir des fichiers", onclick: async () => this.ouvrirFichiers(await this.choisirFichiers()), onkeydown: async (e: KeyboardEvent) => {
      if (e.key === "Enter" || e.key === " ") this.ouvrirFichiers(await this.choisirFichiers());
    } },
      el("div", { class: "depot-icone", html: icone("couches", 44) }),
      el("p", { class: "depot-titre", text: "Déposez vos fichiers de fabrication ici" }),
      el("p", { class: "depot-texte", text: "Gerber (RS-274X, X2, X3, et RS-274D avec sa liste d'ouvertures), perçages Excellon, fichiers de job, ODB++ (.tgz, .zip ou dossier), IPC-2581 (.xml), archives ZIP." }),
      el("div", { class: "depot-boutons" },
        el("button", { type: "button", class: "bouton principal", html: `${icone("ouvrir")}<span>Choisir des fichiers</span>`, onclick: async (e: Event) => {
          e.stopPropagation();
          this.ouvrirFichiers(await this.choisirFichiers());
        } }),
        el("button", { type: "button", class: "bouton", html: `${icone("dossier")}<span>Choisir un dossier</span>`, onclick: async (e: Event) => {
          e.stopPropagation();
          this.ouvrirFichiers(await this.choisirFichiers(true));
        } })));
    const exemples = el("div", { class: "exemples" }, ...EXEMPLES.map((x) => el("button", { type: "button", class: "carte-exemple", onclick: () => this.lancerExemple(x.fichier) },
      el("span", { class: "carte-icone", html: icone(x.icone, 26) }),
      el("span", { class: "carte-texte" }, el("strong", { text: x.titre }), el("span", { text: x.texte })))));
    const atouts = el("ul", { class: "atouts" },
      ...[
        ["Tous les formats", "Gerber RS-274D, X1, X2 et X3, Excellon et XNC, fichiers de job, ODB++ 7 et 8, IPC-2581 A, B et C, archives ZIP et TGZ."],
        ["Comparaison précise", "Superposition, côte à côte ou en alternance, zones de différence listées et rapport PDF."],
        ["ODB++ et IPC-2581 lisibles", "Couches, composants, nets, nomenclature et perçages en tableaux."],
        ["Confidentiel", "Tout est calculé dans votre navigateur par un moteur en WebAssembly : rien n'est envoyé."],
      ].map(([t, x]) => el("li", {}, el("strong", { text: t }), el("span", { text: x }))));
    return el("div", { class: "accueil" },
      el("div", { class: "accueil-contenu" },
        el("div", { class: "accueil-titre" },
          el("h1", {}, el("span", { class: "degrade", text: "Typon" }), " : Visionneuse et comparateur de fichiers de fabrication de circuits imprimés"),
          el("p", { class: "sous-titre", text: "Gerber, perçages, ODB++ et IPC-2581, directement dans le navigateur." })),
        depot,
        el("h2", { class: "titre-exemples", text: "Pour essayer" }),
        exemples,
        atouts));
  }

  private async lancerExemple(fichier: string) {
    if (fichier === "comparer") {
      await this.ouvrirExemple("exemples/typon-demo-revA.zip", "A");
      this.comparaison = { paires: [], choisie: 0, affichage: "superposition", montrerB: false, zone: -1, decalage: { dx: 0, dy: 0 }, aligner: true, tolerance: 0.05, analyse: false };
      await this.ouvrirExemple("exemples/typon-demo-revB.zip", "B");
      return;
    }
    await this.ouvrirExemple(fichier, "A");
  }

  private fermerTout() {
    if (this.visionneuse) {
      this.docA?.fermer(this.visionneuse.rendu);
      this.docB?.fermer(this.visionneuse.rendu);
    }
    this.docA = this.docB = null;
    this.comparaison = null;
    this.selection = null;
    this.afficher();
    this.message("");
  }

  /* --- Couches --------------------------------------------------------------------------------------------------- */

  /** Couches dans l'ordre d'affichage (de la plus haute à la plus basse, selon le côté vu). */
  private ordre(couches: EtatCouche[]): EtatCouche[] {
    const dessus = (c: EtatCouche) => c.desc.role.genre === "percage" || c.desc.role.genre === "fraisage" || c.desc.role.genre === "contour";
    return [...couches].sort((a, b) => {
      if (this.cote === "dessous" && dessus(a) === dessus(b)) return a.desc.role.rang - b.desc.role.rang;
      return b.desc.role.rang - a.desc.role.rang;
    });
  }

  private afficherPanneauGauche() {
    if (!this.docA) return;
    if (this.comparaison) {
      this.panneauGauche.replaceChildren(this.listePaires());
      return;
    }
    const doc = this.docA;
    const prereglages = el("div", { class: "prereglages" }, ...([["dessus", "Dessus"], ["dessous", "Dessous"], ["cuivre", "Cuivre"], ["tout", "Tout"]] as const).map(([p, t]) => el("button", { type: "button", class: "puce", text: t, onclick: () => {
      doc.preregler(p);
      if (p === "dessus" || p === "dessous") this.choisirCote(p);
      this.afficherPanneauGauche();
      this.chargerVisibles();
      this.vue().redessiner();
    } })));
    const liste = el("ul", { class: "liste-couches" });
    for (const c of this.ordre(doc.couches)) liste.append(this.ligneCouche(c));
    this.panneauGauche.replaceChildren(el("div", { class: "titre-panneau" }, el("h2", { html: `${icone("couches", 16)} Couches` }), el("span", { class: "compte", text: String(doc.couches.length) })), prereglages, liste);
  }

  private ligneCouche(c: EtatCouche): HTMLElement {
    const vide = c.desc.objets === 0;
    const oeil = el("button", { type: "button", class: `oeil ${c.visible ? "visible" : ""}`, "aria-pressed": String(c.visible), title: c.visible ? "Masquer" : "Afficher", html: icone(c.visible ? "oeil" : "oeilFerme", 16), onclick: () => {
      c.visible = !c.visible;
      this.chargerVisibles();
      this.afficherPanneauGauche();
      this.vue().redessiner();
    } });
    const pastille = el("input", { type: "color", class: "pastille-couleur", value: c.couleur, title: "Couleur de la couche", "aria-label": `Couleur de ${c.desc.nom}` });
    pastille.addEventListener("input", () => {
      c.couleur = pastille.value;
      this.vue().redessiner();
    });
    const opacite = el("input", { type: "range", min: "0.1", max: "1", step: "0.05", value: String(c.opacite), class: "opacite", title: "Opacité", "aria-label": `Opacité de ${c.desc.nom}` });
    opacite.addEventListener("input", () => {
      c.opacite = Number(opacite.value);
      this.vue().redessiner();
    });
    const nom = el("button", { type: "button", class: "nom-couche", title: `${c.desc.fichier}\n${pluriel(c.desc.objets, "objet")} — cliquez pour les détails`, onclick: () => {
      this.ongletActif = `couche:${c.desc.id}`;
      this.afficherPanneauDroit();
    } }, el("span", { class: "libelle", text: c.desc.nom }), el("span", { class: "fichier", text: vide ? "vide" : c.desc.fichier.split("/").pop()! }));
    return el("li", { class: `couche ${c.visible ? "" : "masquee"} ${vide ? "vide" : ""}`, style: { "--couleur": c.couleur } as Partial<CSSStyleDeclaration> }, oeil, pastille, nom, opacite);
  }

  private chargerVisibles() {
    const docs = [this.docA, this.docB].filter((d): d is DocumentTypon => !!d);
    for (const d of docs) {
      for (const c of d.couches) {
        if (c.visible && !c.gpu) {
          d.gpu(this.vue().rendu, c).then(() => this.vue().redessiner(), (e) => this.message(`Couche « ${c.desc.nom} » : ${(e as Error).message}`, "erreur"));
        }
      }
    }
  }

  private choisirCote(cote: Cote) {
    if (this.cote === cote) return;
    this.cote = cote;
    this.vue().vue = { ...this.vue().vue, miroir: cote === "dessous" };
    for (const d of [this.docA, this.docB]) d?.preregler(cote);
    this.afficherBarre();
    this.afficherPanneauGauche();
    this.chargerVisibles();
    this.vue().redessiner();
  }

  private retourner() {
    this.choisirCote(this.cote === "dessus" ? "dessous" : "dessus");
  }

  private choisirOutil(o: Outil) {
    this.outil = o;
    this.mesure = { panneau: 0 };
    this.vue().racine.classList.toggle("outil-mesure", o === "mesure");
    this.afficherBarre();
    this.vue().redessiner();
    if (o === "mesure") this.message("Mesure : cliquez sur deux points de la carte.");
  }

  /* --- Scènes --------------------------------------------------------------------------------------------------- */

  private affichees(couches: EtatCouche[], decalage = { dx: 0, dy: 0 }): CoucheAffichee[] {
    return this.ordre(couches).reverse().filter((c) => c.visible && c.gpu).map((c) => ({ gpu: c.gpu!, couleur: couleurRgb(c.couleur), opacite: c.opacite, dx: decalage.dx, dy: decalage.dy }));
  }

  private panneaux(): Panneau[] {
    if (!this.docA) return [];
    const cmp = this.comparaison;
    if (!cmp || !this.docB) return [{ de: 0, a: 1, scene: { couches: this.affichees(this.docA.couches) } }];
    const dec = cmp.decalage;
    const paire = cmp.paires[cmp.choisie];
    const contourA = this.docA.couches.filter((c) => c.desc.role.genre === "contour" && c.gpu);
    if (cmp.affichage === "superposition") {
      if (!paire) return [{ de: 0, a: 1, scene: { couches: [] } }];
      const enCouche = (c: EtatCouche | undefined, d: { dx: number; dy: number }) => (c?.gpu ? [{ gpu: c.gpu, couleur: [1, 1, 1] as [number, number, number], opacite: 1, dx: d.dx, dy: d.dy }] : []);
      return [{ de: 0, a: 1, scene: { couches: [], difference: { a: enCouche(paire.a, { dx: 0, dy: 0 }), b: enCouche(paire.b, dec) } } }];
    }
    // Côte à côte et alternance : couches visibles des paires (même visibilité pour A et B).
    const visibles = cmp.paires.filter((p) => (p.a ?? p.b)!.visible);
    const de = (lequel: "a" | "b") => visibles.map((p) => p[lequel]).filter((c): c is EtatCouche => !!c);
    const sceneA = { couches: [...this.affichees(de("a")), ...(de("a").some((c) => c.desc.role.genre === "contour") ? [] : this.affichees(contourA.map((c) => ({ ...c, visible: true }))))] };
    const sceneB = { couches: this.affichees(de("b"), dec) };
    if (cmp.affichage === "alterner") return [{ de: 0, a: 1, scene: cmp.montrerB ? sceneB : sceneA, titre: cmp.montrerB ? `B — ${this.docB.nom}` : `A — ${this.docA.nom}` }];
    return [
      { de: 0, a: 0.5, scene: sceneA, titre: `A — ${this.docA.nom}` },
      { de: 0.5, a: 1, scene: sceneB, titre: `B — ${this.docB.nom}` },
    ];
  }

  private dessinerSurcouche(ctx: CanvasRenderingContext2D, versEcran: (x: number, y: number, panneau?: number) => [number, number]) {
    const r = window.devicePixelRatio || 1;
    const nbPanneaux = this.panneaux().length;
    // Zones de différence
    const cmp = this.comparaison;
    const paire = cmp?.paires[cmp.choisie];
    if (cmp && paire?.zones.length) {
      for (let p = 0; p < nbPanneaux; p++) {
        paire.zones.forEach((z, i) => {
          const [x0, y0] = versEcran(z.x0, z.y1, p);
          const [x1, y1] = versEcran(z.x1, z.y0, p);
          const courant = i === cmp.zone;
          ctx.strokeStyle = courant ? "#ffd166" : "rgba(255, 209, 102, 0.55)";
          ctx.lineWidth = (courant ? 2.5 : 1.2) * r;
          ctx.setLineDash(courant ? [] : [4 * r, 3 * r]);
          const m = 3 * r;
          ctx.strokeRect(Math.min(x0, x1) - m, Math.min(y0, y1) - m, Math.abs(x1 - x0) + 2 * m, Math.abs(y1 - y0) + 2 * m);
          if (courant || paire.zones.length < 60) {
            ctx.setLineDash([]);
            ctx.font = `${600} ${11 * r}px Inter Variable, system-ui, sans-serif`;
            ctx.fillStyle = courant ? "#ffd166" : "rgba(255, 209, 102, 0.8)";
            ctx.fillText(String(i + 1), Math.min(x0, x1) - m, Math.min(y0, y1) - m - 4 * r);
          }
        });
      }
      ctx.setLineDash([]);
    }
    // Objet sélectionné
    if (this.selection) {
      const b = this.selection.objet.boite;
      for (let p = 0; p < nbPanneaux; p++) {
        const [x0, y0] = versEcran(b[0], b[3], p);
        const [x1, y1] = versEcran(b[2], b[1], p);
        const m = 4 * r;
        ctx.strokeStyle = "#5ee7ff";
        ctx.lineWidth = 2 * r;
        ctx.shadowColor = "#5ee7ff";
        ctx.shadowBlur = 8 * r;
        ctx.strokeRect(Math.min(x0, x1) - m, Math.min(y0, y1) - m, Math.abs(x1 - x0) + 2 * m, Math.abs(y1 - y0) + 2 * m);
        ctx.shadowBlur = 0;
      }
    }
    // Mesure
    const { a, b } = this.mesure;
    if (a) {
      for (let p = 0; p < nbPanneaux; p++) {
        const [ax, ay] = versEcran(a.x, a.y, p);
        ctx.fillStyle = "#ff6bd6";
        ctx.beginPath();
        ctx.arc(ax, ay, 4 * r, 0, Math.PI * 2);
        ctx.fill();
        if (b) {
          const [bx, by] = versEcran(b.x, b.y, p);
          ctx.strokeStyle = "#ff6bd6";
          ctx.lineWidth = 2 * r;
          ctx.setLineDash([6 * r, 4 * r]);
          ctx.beginPath();
          ctx.moveTo(ax, ay);
          ctx.lineTo(bx, by);
          ctx.stroke();
          ctx.setLineDash([]);
          ctx.beginPath();
          ctx.arc(bx, by, 4 * r, 0, Math.PI * 2);
          ctx.fill();
          const d = Math.hypot(b.x - a.x, b.y - a.y);
          const texte = `${longueur(d, this.unite)}   Δx ${longueur(Math.abs(b.x - a.x), this.unite)}   Δy ${longueur(Math.abs(b.y - a.y), this.unite)}`;
          ctx.font = `600 ${12 * r}px Inter Variable, system-ui, sans-serif`;
          const w = ctx.measureText(texte).width + 16 * r;
          const mx = (ax + bx) / 2;
          const my = (ay + by) / 2 - 18 * r;
          ctx.fillStyle = "rgba(20, 12, 28, 0.88)";
          ctx.beginPath();
          ctx.roundRect(mx - w / 2, my - 13 * r, w, 24 * r, 6 * r);
          ctx.fill();
          ctx.fillStyle = "#ffd1f3";
          ctx.textAlign = "center";
          ctx.fillText(texte, mx, my + 4 * r);
          ctx.textAlign = "start";
        }
      }
    }
    // Barre d'échelle
    this.dessinerEchelle(ctx);
  }

  private dessinerEchelle(ctx: CanvasRenderingContext2D) {
    const v = this.vue();
    const r = window.devicePixelRatio || 1;
    const cible = 120 * r;
    const brut = cible / v.vue.echelle;
    const facteur = this.unite === "mil" ? 0.0254 : this.unite === "pouce" ? 25.4 : 1;
    const enUnite = brut / facteur;
    const p = 10 ** Math.floor(Math.log10(enUnite));
    const pas = [1, 2, 5, 10].map((k) => k * p).filter((k) => k <= enUnite).pop() ?? p;
    const w = pas * facteur * v.vue.echelle;
    const x = v.canvas.width - w - 20 * r;
    const y = v.canvas.height - 22 * r;
    ctx.strokeStyle = "rgba(230,236,245,0.75)";
    ctx.fillStyle = ctx.strokeStyle;
    ctx.lineWidth = 2 * r;
    ctx.beginPath();
    ctx.moveTo(x, y - 5 * r);
    ctx.lineTo(x, y);
    ctx.lineTo(x + w, y);
    ctx.lineTo(x + w, y - 5 * r);
    ctx.stroke();
    ctx.font = `500 ${11 * r}px Inter Variable, system-ui, sans-serif`;
    ctx.textAlign = "center";
    ctx.fillText(`${nombre(pas, 4)} ${this.unite === "pouce" ? "po" : this.unite}`, x + w / 2, y - 8 * r);
    ctx.textAlign = "start";
  }

  /* --- Pointeur, sélection, mesure ------------------------------------------------------------------------------- */

  private dernierPointeur: Pointeur | null = null;

  private deplacement(p: Pointeur | null) {
    this.dernierPointeur = p;
    this.majHud();
  }

  private majHud() {
    const p = this.dernierPointeur;
    const zoom = this.visionneuse ? `zoom ${nombre(this.visionneuse.vue.echelle / (window.devicePixelRatio || 1), 1)} px/mm` : "";
    this.hud.textContent = p ? `X ${longueur(p.x, this.unite)}   Y ${longueur(p.y, this.unite)}   ·   ${zoom}` : zoom;
    this.visionneuse?.redessiner();
  }

  private async clic(p: Pointeur) {
    if (!this.docA) return;
    this.vue().racine.focus({ preventScroll: true });
    if (this.outil === "mesure") {
      if (!this.mesure.a || this.mesure.b) this.mesure = { a: { x: p.x, y: p.y }, panneau: p.panneau };
      else this.mesure.b = { x: p.x, y: p.y };
      this.vue().redessiner();
      return;
    }
    // Objet sous le pointeur : la couche visible la plus haute d'abord.
    const cmp = this.comparaison;
    let doc = this.docA;
    let couches = this.docA.couches;
    let decalage = { dx: 0, dy: 0 };
    if (cmp && this.docB) {
      const versB = (cmp.affichage === "cote" && p.panneau === 1) || (cmp.affichage === "alterner" && cmp.montrerB);
      if (versB) {
        doc = this.docB;
        decalage = cmp.decalage;
      }
      if (cmp.affichage === "superposition") {
        const paire = cmp.paires[cmp.choisie];
        couches = [paire?.a, paire?.b].filter((c): c is EtatCouche => !!c);
      } else {
        couches = cmp.paires.filter((q) => (q.a ?? q.b)!.visible).map((q) => (doc === this.docB ? q.b : q.a)).filter((c): c is EtatCouche => !!c);
      }
    }
    const tol = 3 / this.vue().vue.echelle * (window.devicePixelRatio || 1);
    // Priorité : le cuivre (du côté regardé d'abord), puis composants, perçages, sérigraphie, contour, masque, pâte.
    const priorite: Record<string, number> = { cuivre: 0, composants: 1, percage: 2, fraisage: 2, serigraphie: 3, contour: 4, masque: 5, pate: 6 };
    const ordonnees = this.ordre(couches).map((c, i) => ({ c, i })).sort((x, y) => (priorite[x.c.desc.role.genre] ?? 7) - (priorite[y.c.desc.role.genre] ?? 7) || x.i - y.i).map((x) => x.c);
    for (const c of ordonnees) {
      if (!c.visible && !cmp) continue;
      const proprio = this.docB?.couches.includes(c) ? this.docB : doc;
      const d = proprio === this.docB ? (cmp?.decalage ?? decalage) : { dx: 0, dy: 0 };
      const r = await moteur.objet(proprio.lot, c.desc.id, p.x - d.dx, p.y - d.dy, tol);
      if (r) {
        r.boite = [r.boite[0] + d.dx, r.boite[1] + d.dy, r.boite[2] + d.dx, r.boite[3] + d.dy];
        this.selection = { doc: proprio, couche: c, objet: r, panneau: p.panneau };
        this.ongletActif = "inspecteur";
        this.afficherPanneauDroit();
        this.vue().redessiner();
        return;
      }
    }
    this.selection = null;
    if (this.ongletActif === "inspecteur") this.afficherPanneauDroit();
    this.vue().redessiner();
  }

  allerA(x: number, y: number, taille = 6) {
    this.vue().ajuster([x - taille, y - taille, x + taille, y + taille], 0);
  }

  /* --- Panneau de droite ----------------------------------------------------------------------------------------- */

  private afficherPanneauDroit() {
    if (!this.docA) return;
    const doc = this.docA;
    const liste: Onglet[] = [];
    if (this.comparaison) {
      liste.push({ id: "differences", titre: "Différences", icone: "comparer", compte: this.comparaison.paires[this.comparaison.choisie]?.zones.length, contenu: () => this.panneauDifferences() });
    }
    liste.push({ id: "inspecteur", titre: "Inspecteur", icone: "info", contenu: () => this.inspecteur() });
    liste.push({ id: "carte", titre: "Carte", icone: "carte", contenu: () => resume(doc, this.unite) });
    liste.push({ id: "fichiers", titre: "Fichiers", icone: "fichier", compte: doc.couches.length, contenu: () => this.panneauFichiers(doc) });
    liste.push(...onglets(doc, this.unite, (x, y) => this.allerA(x, y)));
    if (this.ongletActif.startsWith("couche:")) {
      const id = Number(this.ongletActif.slice(7));
      const c = doc.couches.find((x) => x.desc.id === id);
      if (c) liste.push({ id: this.ongletActif, titre: c.desc.nom, icone: "couches", contenu: () => detailsCouche(c.desc, this.unite) });
    }
    if (!liste.some((o) => o.id === this.ongletActif)) this.ongletActif = this.comparaison ? "differences" : "carte";
    const actif = liste.find((o) => o.id === this.ongletActif)!;
    const nav = el("nav", { class: "onglets", role: "tablist" }, ...liste.filter((o) => !o.id.startsWith("couche:")).map((o) => el("button", { type: "button", role: "tab", class: `onglet ${o.id === actif.id ? "actif" : ""}`, "aria-selected": String(o.id === actif.id), title: o.titre, onclick: () => {
      this.ongletActif = o.id;
      this.afficherPanneauDroit();
    }, html: `${icone(o.icone, 16)}<span>${o.titre}</span>${o.compte ? `<em>${nombre(o.compte, 0)}</em>` : ""}` })));
    const titre = actif.id.startsWith("couche:") ? el("div", { class: "titre-panneau" }, el("button", { type: "button", class: "bouton discret", html: icone("gauche", 16), title: "Retour", onclick: () => {
      this.ongletActif = "fichiers";
      this.afficherPanneauDroit();
    } }), el("h2", { text: actif.titre })) : null;
    this.panneauDroit.replaceChildren(nav, el("div", { class: "contenu-onglet" }, titre, actif.contenu()));
  }

  private inspecteur(): HTMLElement {
    const s = this.selection;
    if (!s) {
      return el("div", { class: "invite" }, el("div", { class: "invite-icone", html: icone("info", 36) }), el("p", { text: "Cliquez sur un élément de la carte (piste, pastille, zone, perçage, composant) pour voir ses informations : ouverture, net, composant, broche, attributs." }), el("p", { class: "discret", text: "Raccourcis : F ajuster, B retourner la carte, M mesurer, Échap effacer." }));
    }
    const b = s.objet.boite;
    const dl = el("dl", { class: "definitions" });
    dl.append(el("dt", { text: "Couche" }), el("dd", { text: s.couche.desc.nom }));
    if (this.comparaison) dl.append(el("dt", { text: "Version" }), el("dd", { text: s.doc === this.docB ? "B" : "A" }));
    for (const [k, v] of s.objet.info) dl.append(el("dt", { text: k }), el("dd", { text: v }));
    dl.append(el("dt", { text: "Centre" }), el("dd", { text: `X ${longueur((b[0] + b[2]) / 2, this.unite)} ; Y ${longueur((b[1] + b[3]) / 2, this.unite)}` }));
    dl.append(el("dt", { text: "Encombrement" }), el("dd", { text: `${longueur(b[2] - b[0], this.unite)} × ${longueur(b[3] - b[1], this.unite)}` }));
    return el("div", {}, el("section", { class: "bloc" }, el("h3", { text: "Objet sélectionné" }), dl));
  }

  private panneauFichiers(doc: DocumentTypon): HTMLElement {
    const ul = el("ul", { class: "liste-fichiers" });
    for (const c of this.ordre(doc.couches)) {
      ul.append(el("li", {}, el("button", { type: "button", onclick: () => {
        this.ongletActif = `couche:${c.desc.id}`;
        this.afficherPanneauDroit();
      } }, el("span", { class: "puce-couleur", style: { background: c.couleur } }), el("span", { class: "fichier-nom" }, el("strong", { text: c.desc.nom }), el("span", { text: `${c.desc.fichier.split("/").pop()} · ${c.desc.meta.format ?? c.desc.format}${c.desc.meta.alertes?.length ? " · ⚠" : ""}` })), el("span", { class: "fichier-compte", text: nombre(c.desc.objets, 0) }))));
    }
    return el("div", {}, el("p", { class: "discret", text: "Cliquez sur une couche pour voir son format, ses attributs X2, ses ouvertures ou ses outils." }), ul);
  }

  /* --- Comparaison ------------------------------------------------------------------------------------------------ */

  private demarrerComparaison() {
    if (!this.docA || !this.docB) return;
    const ancien = this.comparaison;
    const paires = apparier(this.docA, this.docB);
    const decalage = alignementAuto(this.docA, this.docB);
    this.comparaison = { paires, choisie: Math.max(0, paires.findIndex((p) => p.a && p.b && p.a.desc.role.genre === "cuivre")), affichage: ancien?.affichage ?? "superposition", montrerB: false, zone: -1, decalage, aligner: true, tolerance: ancien?.tolerance ?? 0.05, analyse: false };
    for (const d of [this.docA, this.docB]) d.preregler(this.cote);
    this.ongletActif = "differences";
    this.selection = null;
    for (const p of paires) {
      const v = (p.a ?? p.b)!.visible;
      if (p.a) p.a.visible = v;
      if (p.b) p.b.visible = v;
    }
    void this.analyserTout();
  }

  private async analyserTout() {
    const cmp = this.comparaison;
    if (!cmp || !this.docA || !this.docB || cmp.analyse) return;
    cmp.analyse = true;
    const a = this.docA;
    const b = this.docB;
    // Couches de la paire choisie d'abord, pour un premier résultat rapide.
    const ordre = [cmp.paires[cmp.choisie], ...cmp.paires.filter((_, i) => i !== cmp.choisie)].filter((p) => p && p.a && p.b && p.etat === "a-analyser");
    let n = 0;
    for (const p of ordre) {
      if (this.comparaison !== cmp) return;
      this.message(`Recherche des différences : ${p.libelle} (${++n} sur ${ordre.length})…`);
      try {
        await analyser(this.vue().rendu, a, b, p, cmp.aligner ? cmp.decalage : { dx: 0, dy: 0 }, cmp.tolerance);
      } catch (e) {
        console.error(e);
        p.etat = "a-analyser";
      }
      if (p === cmp.paires[cmp.choisie] && p.zones.length && cmp.zone < 0) cmp.zone = -1;
      this.afficherPanneauGauche();
      if (p === cmp.paires[cmp.choisie]) this.afficherPanneauDroit();
      this.vue().redessiner();
    }
    cmp.analyse = false;
    const avec = cmp.paires.filter((p) => p.etat === "differences");
    const seules = cmp.paires.filter((p) => p.etat === "seulement-a" || p.etat === "seulement-b");
    this.message(avec.length || seules.length
      ? `${pluriel(avec.length, "couche")} avec des différences${seules.length ? `, ${pluriel(seules.length, "couche")} présente${seules.length > 1 ? "s" : ""} dans une seule version` : ""}.`
      : "Aucune différence : les deux versions sont identiques (à la tolérance près).", avec.length || seules.length ? "info" : "succes");
    this.chargerVisibles();
  }

  private listePaires(): HTMLElement {
    const cmp = this.comparaison!;
    const ul = el("ul", { class: "liste-couches paires" });
    if (!this.docB) {
      return el("div", { class: "invite", "data-depot": "B" }, el("div", { class: "invite-icone", html: icone("comparer", 36) }), el("p", { text: "Déposez ici la version B (nouvelle version) : Gerber, archive ZIP, ODB++ ou IPC-2581. Elle peut être dans un autre format que la version A." }), el("button", { type: "button", class: "bouton principal", html: `${icone("ouvrir")}<span>Choisir la version B</span>`, onclick: async () => this.ouvrirFichiers(await this.choisirFichiers(), "B") }));
    }
    cmp.paires.forEach((p, i) => {
      const ref = (p.a ?? p.b)!;
      const badge = {
        "a-analyser": ["attente", "…"],
        analyse: ["attente", "analyse…"],
        identique: ["ok", "identique"],
        differences: ["diff", pluriel(p.zones.length, "zone")],
        "seulement-a": ["seul", "seulement A"],
        "seulement-b": ["seul", "seulement B"],
        representation: ["seul", "voir la liste"],
      }[p.etat];
      const oeil = el("button", { type: "button", class: `oeil ${ref.visible ? "visible" : ""}`, "aria-pressed": String(ref.visible), title: "Afficher en côte à côte et en alternance", html: icone(ref.visible ? "oeil" : "oeilFerme", 16), onclick: (e: Event) => {
        e.stopPropagation();
        const v = !ref.visible;
        if (p.a) p.a.visible = v;
        if (p.b) p.b.visible = v;
        this.chargerVisibles();
        this.afficherPanneauGauche();
        this.vue().redessiner();
      } });
      ul.append(el("li", { class: `couche paire ${i === cmp.choisie ? "choisie" : ""} etat-${p.etat}`, style: { "--couleur": ref.couleur } as Partial<CSSStyleDeclaration> },
        oeil,
        el("button", { type: "button", class: "nom-couche", onclick: () => this.choisirPaire(i), title: `A : ${p.a?.desc.fichier ?? "—"}\nB : ${p.b?.desc.fichier ?? "—"}` },
          el("span", { class: "libelle", text: p.libelle }),
          el("span", { class: "fichier", text: `${p.a?.desc.format ?? "—"} / ${p.b?.desc.format ?? "—"}` })),
        el("span", { class: `badge ${badge[0]}`, text: badge[1] })));
    });
    const reglages = el("div", { class: "reglages-comparaison" });
    const aligner = el("label", { class: "case" }, el("input", { type: "checkbox", checked: cmp.aligner, onchange: (e: Event) => {
      cmp.aligner = (e.target as HTMLInputElement).checked;
      this.relancerAnalyse();
    } }), el("span", { text: cmp.decalage.dx || cmp.decalage.dy ? `Aligner les origines (B décalée de ${longueur(cmp.decalage.dx, this.unite, 3)} ; ${longueur(cmp.decalage.dy, this.unite, 3)})` : "Aligner les origines (identiques)" }));
    if (!(cmp.decalage.dx || cmp.decalage.dy)) aligner.querySelector("input")!.disabled = true;
    const tolerance = el("label", { class: "champ" }, el("span", { text: "Ignorer les écarts de moins de" }), el("select", { onchange: (e: Event) => {
      cmp.tolerance = Number((e.target as HTMLSelectElement).value);
      this.relancerAnalyse();
    } }, ...[0, 0.01, 0.02, 0.05, 0.1, 0.2].map((t) => el("option", { value: String(t), text: t ? longueur(t, this.unite, 3) : "rien (exact)", selected: t === cmp.tolerance }))));
    reglages.append(aligner, tolerance);
    return el("div", {}, el("div", { class: "titre-panneau" }, el("h2", { html: `${icone("comparer", 16)} Couches comparées` }), el("span", { class: "compte", text: String(cmp.paires.length) })), ul, reglages);
  }

  private relancerAnalyse() {
    const cmp = this.comparaison;
    if (!cmp) return;
    for (const p of cmp.paires) if (p.a && p.b && p.etat !== "representation") {
      p.etat = "a-analyser";
      p.zones = [];
    }
    cmp.zone = -1;
    this.afficherPanneauGauche();
    void this.analyserTout();
  }

  private choisirPaire(i: number) {
    const cmp = this.comparaison!;
    cmp.choisie = i;
    cmp.zone = -1;
    const p = cmp.paires[i];
    for (const c of [p.a, p.b]) {
      if (!c) continue;
      const d = this.docA!.couches.includes(c) ? this.docA! : this.docB!;
      void d.gpu(this.vue().rendu, c).then(() => this.vue().redessiner());
    }
    this.ongletActif = "differences";
    this.afficherPanneauGauche();
    this.afficherPanneauDroit();
    this.vue().redessiner();
  }

  private choisirAffichage(m: Affichage) {
    if (!this.comparaison) return;
    this.comparaison.affichage = m;
    this.afficherBarre();
    this.chargerVisibles();
    this.vue().redessiner();
  }

  private echanger() {
    if (!this.docA || !this.docB) return;
    [this.docA, this.docB] = [this.docB, this.docA];
    this.demarrerComparaison();
    this.afficher();
  }

  private allerZone(i: number) {
    const cmp = this.comparaison;
    const p = cmp?.paires[cmp.choisie];
    if (!cmp || !p || !p.zones.length) return;
    cmp.zone = (i + p.zones.length) % p.zones.length;
    const z = p.zones[cmp.zone];
    const m = Math.max(1.5, (z.x1 - z.x0) * 0.6, (z.y1 - z.y0) * 0.6);
    this.vue().ajuster([z.x0 - m, z.y0 - m, z.x1 + m, z.y1 + m], 0);
    this.afficherPanneauDroit();
  }

  private panneauDifferences(): HTMLElement {
    const cmp = this.comparaison!;
    const p = cmp.paires[cmp.choisie];
    const racine = el("div");
    if (!this.docB) return el("p", { class: "vide", text: "Ouvrez la version B pour lancer la comparaison." });
    racine.append(el("div", { class: "legende" },
      el("span", {}, el("i", { style: { background: "#ff4545" } }), "Seulement dans A (disparu)"),
      el("span", {}, el("i", { style: { background: "#40eb73" } }), "Seulement dans B (apparu)"),
      el("span", {}, el("i", { style: { background: "#7f8c9e" } }), "Commun")));
    const actions = el("div", { class: "actions-differences" },
      el("button", { type: "button", class: "bouton principal", html: `${icone("pdf")}<span>Rapport PDF</span>`, onclick: () => this.rapport(), disabled: cmp.analyse }));
    racine.append(actions);
    if (!p) return racine;
    racine.append(el("h3", { class: "titre-paire", text: p.libelle }));
    if (p.etat === "representation") {
      racine.append(el("p", { class: "vide", text: "Les deux versions représentent les composants différemment (points du Gerber X3, contours des boîtiers en ODB++ ou IPC-2581) : leur dessin n'est pas comparable. La comparaison de la liste des composants est ci-dessous." }));
      racine.append(this.differencesComposants());
      return racine;
    }
    if (p.etat === "seulement-a" || p.etat === "seulement-b") {
      racine.append(el("p", { class: "vide", text: p.etat === "seulement-a" ? "Cette couche n'existe que dans la version A." : "Cette couche n'existe que dans la version B." }));
      return racine;
    }
    if (p.etat === "analyse" || p.etat === "a-analyser") {
      racine.append(el("p", { class: "vide", text: "Recherche des différences…" }));
      return racine;
    }
    if (p.etat === "identique") {
      racine.append(el("div", { class: "identique", html: `${icone("info", 18)}<span>Aucune différence sur cette couche${cmp.tolerance ? ` (écarts de moins de ${longueur(cmp.tolerance, this.unite, 3)} ignorés)` : ""}.</span>` }));
      return racine;
    }
    const nav = el("div", { class: "nav-zones" },
      el("button", { type: "button", class: "bouton", html: `${icone("gauche", 16)}<span>Précédente</span>`, title: "Zone précédente (P)", onclick: () => this.allerZone(cmp.zone - 1) }),
      el("span", { text: cmp.zone >= 0 ? `Zone ${cmp.zone + 1} sur ${p.zones.length}` : pluriel(p.zones.length, "zone") }),
      el("button", { type: "button", class: "bouton", html: `<span>Suivante</span>${icone("droite", 16)}`, title: "Zone suivante (N)", onclick: () => this.allerZone(cmp.zone + 1) }));
    racine.append(nav);
    const ul = el("ol", { class: "liste-zones" });
    p.zones.forEach((z: ZoneMm, i) => {
      const genre = z.disparu > 0 && z.apparu > 0 ? "modifié" : z.disparu > 0 ? "disparu" : "apparu";
      ul.append(el("li", { class: i === cmp.zone ? "courante" : "" }, el("button", { type: "button", onclick: () => this.allerZone(i) },
        el("span", { class: "numero", text: String(i + 1) }),
        el("span", { class: `genre ${genre}`, text: genre }),
        el("span", { class: "position", text: `X ${longueur((z.x0 + z.x1) / 2, this.unite, 2)} · Y ${longueur((z.y0 + z.y1) / 2, this.unite, 2)}` }),
        el("span", { class: "dimension", text: `${longueur(z.x1 - z.x0, this.unite, 2)} × ${longueur(z.y1 - z.y0, this.unite, 2)}` }))));
    });
    racine.append(ul);
    if (p.resolution) racine.append(el("p", { class: "discret", text: `Comparaison au pas de ${longueur(p.resolution, this.unite, 3)}.` }));
    racine.append(this.differencesComposants());
    return racine;
  }

  /** Composants ajoutés, retirés ou modifiés entre A et B. */
  private differencesComposants(): HTMLElement {
    const ca = this.docA?.description.meta.composants ?? [];
    const cb = this.docB?.description.meta.composants ?? [];
    const bloc = el("section", { class: "bloc composants-differences" }, el("h3", { text: "Composants" }));
    if (!ca.length && !cb.length) {
      bloc.append(el("p", { class: "discret", text: "Aucune liste de composants (ni Gerber X3, ni ODB++)." }));
      return bloc;
    }
    if (!ca.length || !cb.length) {
      bloc.append(el("p", { class: "discret", text: `Seule la version ${ca.length ? "A" : "B"} contient la liste des composants.` }));
      return bloc;
    }
    const d = comparerComposants(ca, cb, this.comparaison?.aligner ? this.comparaison.decalage : { dx: 0, dy: 0 });
    if (!d.ajoutes.length && !d.retires.length && !d.modifies.length) {
      bloc.append(el("div", { class: "identique" }, el("span", { html: icone("info", 18) }), el("span", { text: `Les ${nombre(d.communs, 0)} composants sont identiques.` })));
      return bloc;
    }
    const ligne = (genre: string, ref: string, texte: string, x?: number, y?: number) => el("li", {}, el("button", { type: "button", disabled: x === undefined, onclick: () => x !== undefined && this.allerA(x, y!, 4) },
      el("span", { class: `genre ${genre}`, text: genre }), el("strong", { text: ref }), el("span", { text: texte })));
    const ul = el("ul", { class: "liste-composants" });
    for (const c of d.retires) ul.append(ligne("disparu", c.reference, [c.valeur, c.boitier].filter(Boolean).join(" · "), c.x, c.y));
    for (const c of d.ajoutes) ul.append(ligne("apparu", c.reference, [c.valeur, c.boitier].filter(Boolean).join(" · "), c.x, c.y));
    for (const m of d.modifies) {
      const c = cb.find((x) => x.reference === m.reference);
      ul.append(ligne("modifié", m.reference, m.changements.join(" ; "), c?.x, c?.y));
    }
    bloc.append(el("p", { class: "discret", text: `${pluriel(d.ajoutes.length, "ajouté")}, ${pluriel(d.retires.length, "retiré")}, ${pluriel(d.modifies.length, "modifié")}, ${pluriel(d.communs - d.modifies.length, "inchangé")}.` }), ul);
    return bloc;
  }

  private async rapport() {
    const cmp = this.comparaison;
    if (!cmp || !this.docA || !this.docB) return;
    const { rapportPdf } = await import("./rapport");
    await this.attendre("Préparation du rapport PDF…", () => rapportPdf({ a: this.docA!, b: this.docB!, paires: cmp.paires, decalage: cmp.aligner ? cmp.decalage : { dx: 0, dy: 0 }, tolerance: cmp.tolerance, unite: this.unite, rendu: this.vue().rendu }));
    this.vue().redessiner();
    this.message("Rapport PDF enregistré (dossier des téléchargements).", "succes");
  }
}

/** Fichiers d'un dépôt, dossiers compris (parcours récursif). */
async function fichiersDeposes(dt: DataTransfer | null): Promise<File[]> {
  if (!dt) return [];
  const entrees = [...dt.items].map((i) => i.webkitGetAsEntry?.()).filter((e): e is FileSystemEntry => !!e);
  if (!entrees.length) return [...dt.files];
  const fichiers: File[] = [];
  const lire = async (e: FileSystemEntry, chemin: string): Promise<void> => {
    if (e.isFile) {
      const f = await new Promise<File>((ok, ko) => (e as FileSystemFileEntry).file(ok, ko));
      Object.defineProperty(f, "chemin", { value: chemin + f.name });
      fichiers.push(f);
    } else if (e.isDirectory) {
      const lecteur = (e as FileSystemDirectoryEntry).createReader();
      for (;;) {
        const lot = await new Promise<FileSystemEntry[]>((ok, ko) => lecteur.readEntries(ok, ko));
        if (!lot.length) break;
        for (const x of lot) await lire(x, `${chemin}${e.name}/`);
      }
    }
  };
  for (const e of entrees) await lire(e, "");
  return fichiers;
}
