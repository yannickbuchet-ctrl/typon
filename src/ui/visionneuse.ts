/**
 * Zone d'affichage : canevas WebGL, surcouche 2D (sélection, mesure, zones), déplacement et zoom à la souris, au
 * doigt (pincement) et au clavier, une ou deux vues synchronisées (comparaison côte à côte).
 */
import { Rendu, type Scene, type Vue } from "../rendu/rendu";
import { el } from "./dom";

export interface Panneau {
  /** Fraction horizontale occupée : [début, fin] entre 0 et 1. */
  de: number;
  a: number;
  scene: Omit<Scene, "x" | "y" | "w" | "h" | "vue">;
  titre?: string;
}

export interface Pointeur {
  /** Position en mm (repère de la carte). */
  x: number;
  y: number;
  /** Indice du panneau sous le pointeur. */
  panneau: number;
}

export class Visionneuse {
  readonly racine: HTMLElement;
  readonly canvas: HTMLCanvasElement;
  readonly surcouche: HTMLCanvasElement;
  readonly rendu: Rendu;
  vue: Vue = { cx: 0, cy: 0, echelle: 4, miroir: false };
  fond: [number, number, number] = [0.04, 0.055, 0.075];
  panneaux: () => Panneau[] = () => [];
  dessinerSurcouche: (ctx: CanvasRenderingContext2D, versEcran: (x: number, y: number, panneau?: number) => [number, number]) => void = () => {};
  surDeplacement: (p: Pointeur | null) => void = () => {};
  surClic: (p: Pointeur) => void = () => {};
  surChangementVue: () => void = () => {};
  private demande = 0;
  /** Cadrage demandé avant que le canevas ait sa taille : appliqué au premier redimensionnement. */
  private cadrageEnAttente: { boite: [number, number, number, number] | null; marge: number } | null = null;
  private pointeurs = new Map<number, { x: number; y: number }>();
  private glisse: { x: number; y: number; cx: number; cy: number; deplace: boolean } | null = null;
  private pince: { d: number; echelle: number; mx: number; my: number } | null = null;
  private etiquettes: HTMLElement;

  constructor() {
    this.canvas = el("canvas", { class: "gl" });
    this.surcouche = el("canvas", { class: "surcouche" });
    this.etiquettes = el("div", { class: "etiquettes-panneaux" });
    this.racine = el("div", { class: "visionneuse", tabindex: "0" }, this.canvas, this.surcouche, this.etiquettes);
    this.rendu = new Rendu(this.canvas);
    new ResizeObserver(() => this.redimensionner()).observe(this.racine);
    this.installerEvenements();
  }

  get ratio() {
    return window.devicePixelRatio || 1;
  }

  private redimensionner() {
    const r = this.racine.getBoundingClientRect();
    const w = Math.max(1, Math.round(r.width * this.ratio));
    const h = Math.max(1, Math.round(r.height * this.ratio));
    for (const c of [this.canvas, this.surcouche]) {
      if (c.width !== w || c.height !== h) {
        c.width = w;
        c.height = h;
      }
    }
    if (this.cadrageEnAttente && r.width > 2 && r.height > 2) {
      const { boite, marge } = this.cadrageEnAttente;
      this.cadrageEnAttente = null;
      this.ajuster(boite, marge);
    }
    this.redessiner();
  }

  /** Rectangle (pixels de l'appareil, origine en haut à gauche) d'un panneau. */
  private rect(p: Panneau) {
    const W = this.canvas.width;
    const x = Math.round(p.de * W);
    return { x, y: 0, w: Math.round(p.a * W) - x, h: this.canvas.height };
  }

  redessiner() {
    if (this.demande) return;
    this.demande = requestAnimationFrame(() => {
      this.demande = 0;
      this.dessinerMaintenant();
    });
  }

  dessinerMaintenant() {
    const panneaux = this.panneaux();
    const H = this.canvas.height;
    const scenes: Scene[] = panneaux.map((p) => {
      const r = this.rect(p);
      return { ...p.scene, x: r.x, y: H - r.y - r.h, w: r.w, h: r.h, vue: this.vue };
    });
    this.rendu.dessiner(scenes, this.fond);
    const ctx = this.surcouche.getContext("2d")!;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, this.surcouche.width, this.surcouche.height);
    // Séparation entre deux panneaux.
    for (const p of panneaux.slice(1)) {
      const r = this.rect(p);
      ctx.fillStyle = "rgba(255,255,255,0.14)";
      ctx.fillRect(r.x - Math.round(this.ratio), 0, Math.max(1, Math.round(2 * this.ratio)), H);
    }
    this.dessinerSurcouche(ctx, (x, y, i = 0) => this.versEcran(x, y, panneaux[i] ?? panneaux[0]));
    this.etiquettes.replaceChildren(...panneaux.filter((p) => p.titre).map((p) => el("span", { class: "etiquette-panneau", style: { left: `${p.de * 100}%`, width: `${(p.a - p.de) * 100}%` } }, el("span", { text: p.titre! }))));
  }

  /** mm -> pixels de l'appareil (origine en haut à gauche du canevas). */
  versEcran(x: number, y: number, p?: Panneau): [number, number] {
    const r = p ? this.rect(p) : { x: 0, y: 0, w: this.canvas.width, h: this.canvas.height };
    const sx = (this.vue.miroir ? -1 : 1) * this.vue.echelle;
    return [r.x + r.w / 2 + (x - this.vue.cx) * sx, r.y + r.h / 2 - (y - this.vue.cy) * this.vue.echelle];
  }

  /** Pixels CSS (relatifs au canevas) -> mm, et panneau concerné. */
  versCarte(px: number, py: number): Pointeur {
    const dx = px * this.ratio;
    const dy = py * this.ratio;
    const panneaux = this.panneaux();
    let i = panneaux.findIndex((p) => {
      const r = this.rect(p);
      return dx >= r.x && dx < r.x + r.w;
    });
    if (i < 0) i = 0;
    const r = panneaux[i] ? this.rect(panneaux[i]) : { x: 0, y: 0, w: this.canvas.width, h: this.canvas.height };
    const sx = (this.vue.miroir ? -1 : 1) * this.vue.echelle;
    return { x: this.vue.cx + (dx - r.x - r.w / 2) / sx, y: this.vue.cy - (dy - r.y - r.h / 2) / this.vue.echelle, panneau: i };
  }

  /** Cadre la boîte (mm) dans le premier panneau, avec une marge. */
  ajuster(boite: [number, number, number, number] | null, marge = 0.06) {
    const taille = this.racine.getBoundingClientRect();
    // Le canevas prend la taille de sa zone avant le calcul (l'observateur de taille n'est peut-être pas encore passé).
    if (taille.width > 2 && taille.height > 2) {
      const w = Math.round(taille.width * this.ratio);
      const h = Math.round(taille.height * this.ratio);
      for (const c of [this.canvas, this.surcouche]) {
        if (c.width !== w || c.height !== h) {
          c.width = w;
          c.height = h;
        }
      }
    }
    if (taille.width <= 2 || taille.height <= 2) {
      this.cadrageEnAttente = { boite, marge };
      return;
    }
    if (!boite || !(boite[2] > boite[0]) || !(boite[3] > boite[1])) {
      this.vue = { ...this.vue, cx: 0, cy: 0, echelle: 4 * this.ratio };
    } else {
      const p = this.panneaux()[0];
      const r = p ? this.rect(p) : { w: this.canvas.width, h: this.canvas.height };
      const l = (boite[2] - boite[0]) * (1 + 2 * marge);
      const h = (boite[3] - boite[1]) * (1 + 2 * marge);
      this.vue = { ...this.vue, cx: (boite[0] + boite[2]) / 2, cy: (boite[1] + boite[3]) / 2, echelle: Math.max(1e-3, Math.min(r.w / l, r.h / h)) };
    }
    this.surChangementVue();
    this.redessiner();
  }

  /** Zoom d'un facteur autour d'un point (pixels CSS). */
  zoomer(facteur: number, px?: number, py?: number) {
    const r = this.racine.getBoundingClientRect();
    const x = px ?? r.width / 2;
    const y = py ?? r.height / 2;
    const avant = this.versCarte(x, y);
    const echelle = Math.min(2e5, Math.max(1e-2, this.vue.echelle * facteur));
    this.vue = { ...this.vue, echelle };
    const apres = this.versCarte(x, y);
    this.vue = { ...this.vue, cx: this.vue.cx + avant.x - apres.x, cy: this.vue.cy + avant.y - apres.y };
    this.surChangementVue();
    this.redessiner();
  }

  private installerEvenements() {
    const c = this.racine;
    const local = (e: { clientX: number; clientY: number }) => {
      const r = c.getBoundingClientRect();
      return { x: e.clientX - r.left, y: e.clientY - r.top };
    };
    c.addEventListener("wheel", (e) => {
      e.preventDefault();
      const p = local(e);
      // Pavé tactile : le pincement arrive en molette avec Ctrl ; un défilement à deux doigts déplace la vue.
      if (!e.ctrlKey && Math.abs(e.deltaX) > Math.abs(e.deltaY) * 0.5 && e.deltaMode === 0 && Math.abs(e.deltaX) > 0) {
        const sx = (this.vue.miroir ? -1 : 1) * this.vue.echelle;
        this.vue = { ...this.vue, cx: this.vue.cx + (e.deltaX * this.ratio) / sx, cy: this.vue.cy - (e.deltaY * this.ratio) / this.vue.echelle };
        this.surChangementVue();
        this.redessiner();
        return;
      }
      const d = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
      // Pincement d'un pavé tactile : petites valeurs avec Ctrl, zoom fin ; cran de molette (avec ou sans Ctrl) : ×1,2
      // environ par cran. En côte à côte, les deux panneaux partagent la même vue : ils restent synchronisés.
      const pincement = e.ctrlKey && Math.abs(d) < 40;
      this.zoomer(Math.exp(-d * (pincement ? 0.01 : 0.0018)), p.x, p.y);
    }, { passive: false });
    c.addEventListener("pointerdown", (e) => {
      c.setPointerCapture(e.pointerId);
      const p = local(e);
      this.pointeurs.set(e.pointerId, p);
      if (this.pointeurs.size === 1) {
        this.glisse = { x: p.x, y: p.y, cx: this.vue.cx, cy: this.vue.cy, deplace: false };
      } else if (this.pointeurs.size === 2) {
        const [a, b] = [...this.pointeurs.values()];
        this.pince = { d: Math.hypot(a.x - b.x, a.y - b.y), echelle: this.vue.echelle, mx: (a.x + b.x) / 2, my: (a.y + b.y) / 2 };
        this.glisse = null;
      }
    });
    c.addEventListener("pointermove", (e) => {
      const p = local(e);
      if (this.pointeurs.has(e.pointerId)) this.pointeurs.set(e.pointerId, p);
      if (this.pince && this.pointeurs.size === 2) {
        const [a, b] = [...this.pointeurs.values()];
        const d = Math.hypot(a.x - b.x, a.y - b.y);
        const mx = (a.x + b.x) / 2;
        const my = (a.y + b.y) / 2;
        const avant = this.versCarte(this.pince.mx, this.pince.my);
        this.vue = { ...this.vue, echelle: Math.min(2e5, Math.max(1e-2, this.pince.echelle * (d / Math.max(1, this.pince.d)))) };
        const apres = this.versCarte(mx, my);
        this.vue = { ...this.vue, cx: this.vue.cx + avant.x - apres.x, cy: this.vue.cy + avant.y - apres.y };
        this.pince = { ...this.pince, mx, my, echelle: this.vue.echelle, d };
        this.surChangementVue();
        this.redessiner();
        return;
      }
      if (this.glisse) {
        const dx = p.x - this.glisse.x;
        const dy = p.y - this.glisse.y;
        if (Math.hypot(dx, dy) > 4) this.glisse.deplace = true;
        if (this.glisse.deplace) {
          const sx = (this.vue.miroir ? -1 : 1) * this.vue.echelle;
          this.vue = { ...this.vue, cx: this.glisse.cx - (dx * this.ratio) / sx, cy: this.glisse.cy + (dy * this.ratio) / this.vue.echelle };
          this.surChangementVue();
          this.redessiner();
        }
      }
      this.surDeplacement(this.versCarte(p.x, p.y));
    });
    const fin = (e: PointerEvent) => {
      const p = local(e);
      const etaitPince = this.pointeurs.size === 2;
      this.pointeurs.delete(e.pointerId);
      if (this.pointeurs.size < 2) this.pince = null;
      if (this.glisse && !this.glisse.deplace && !etaitPince && e.type === "pointerup") this.surClic(this.versCarte(p.x, p.y));
      this.glisse = null;
    };
    c.addEventListener("pointerup", fin);
    c.addEventListener("pointercancel", fin);
    c.addEventListener("pointerleave", () => this.surDeplacement(null));
    c.addEventListener("keydown", (e) => {
      const pas = 60 * this.ratio / this.vue.echelle;
      const sens = this.vue.miroir ? -1 : 1;
      if (e.key === "+" || e.key === "=") this.zoomer(1.4);
      else if (e.key === "-") this.zoomer(1 / 1.4);
      else if (e.key === "ArrowLeft") this.vue = { ...this.vue, cx: this.vue.cx - pas * sens };
      else if (e.key === "ArrowRight") this.vue = { ...this.vue, cx: this.vue.cx + pas * sens };
      else if (e.key === "ArrowUp") this.vue = { ...this.vue, cy: this.vue.cy + pas };
      else if (e.key === "ArrowDown") this.vue = { ...this.vue, cy: this.vue.cy - pas };
      else return;
      e.preventDefault();
      this.surChangementVue();
      this.redessiner();
    });
  }
}
