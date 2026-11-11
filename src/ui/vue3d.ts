/**
 * Vue 3D de la carte : plaque à l'épaisseur du fichier (ou 1,6 mm), découpée selon le contour, faces texturées par les
 * couches de cuivre, de vernis épargne et de sérigraphie, trous traversants (métallisés ou non) et boîtiers des
 * composants (ODB++, IPC-2581). Rotation, déplacement et zoom à la souris ou au doigt ; clic sur un composant pour ses
 * informations.
 *
 * La vue a son propre canevas et son propre contexte WebGL 2 : les couches y sont peintes une fois dans deux textures
 * (dessus, dessous ; un canal par genre : R cuivre, V ouvertures du vernis, B sérigraphie, A perçages), puis la scène
 * est dessinée avec un tampon de profondeur multi-échantillonné.
 */
import { moteur, type Composant, type Geometrie } from "../moteur/client";
import { Rendu, type CoucheAffichee, type CoucheGpu } from "../rendu/rendu";
import type { DocumentTypon } from "./document";
import { el, nombre } from "./dom";
import { t } from "../i18n";
import { traduireMoteur as tm } from "../i18n-moteur";

type Mat4 = Float32Array;
type Vec3 = [number, number, number];

/* --- Petite algèbre (matrices 4 × 4 en colonnes, comme WebGL) ---------------------------------------------------- */

function multiplier(a: Mat4, b: Mat4): Mat4 {
  const r = new Float32Array(16);
  for (let i = 0; i < 4; i++) for (let j = 0; j < 4; j++) {
    let s = 0;
    for (let k = 0; k < 4; k++) s += a[k * 4 + j] * b[i * 4 + k];
    r[i * 4 + j] = s;
  }
  return r;
}

function perspective(fov: number, aspect: number, pres: number, loin: number): Mat4 {
  const f = 1 / Math.tan(fov / 2);
  const r = new Float32Array(16);
  r[0] = f / aspect;
  r[5] = f;
  r[10] = (loin + pres) / (pres - loin);
  r[11] = -1;
  r[14] = (2 * loin * pres) / (pres - loin);
  return r;
}

const sous = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const croix = (a: Vec3, b: Vec3): Vec3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const scal = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const norm = (a: Vec3): Vec3 => {
  const l = Math.hypot(...a) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
};

function regarder(oeil: Vec3, cible: Vec3, haut: Vec3): Mat4 {
  const z = norm(sous(oeil, cible));
  const x = norm(croix(haut, z));
  const y = croix(z, x);
  return new Float32Array([x[0], y[0], z[0], 0, x[1], y[1], z[1], 0, x[2], y[2], z[2], 0, -scal(x, oeil), -scal(y, oeil), -scal(z, oeil), 1]);
}

/* --- Shaders -------------------------------------------------------------------------------------------------------- */

const ECLAIRAGE = `
uniform vec3 uOeil;
vec3 eclairer(vec3 couleur, vec3 n, vec3 p, float brillance) {
  vec3 v = normalize(uOeil - p);
  if (dot(n, v) < 0.0) n = -n;
  vec3 l1 = normalize(vec3(-0.45, -0.6, 1.0)), l2 = normalize(vec3(0.6, 0.35, -0.55));
  float d = 0.34 + 0.56 * max(dot(n, l1), 0.0) + 0.22 * max(dot(n, l2), 0.0) + 0.12 * max(dot(n, v), 0.0);
  float s = pow(max(dot(reflect(-l1, n), v), 0.0), mix(18.0, 70.0, brillance)) * mix(0.10, 0.85, brillance);
  s += pow(max(dot(reflect(-l2, n), v), 0.0), 40.0) * 0.25 * brillance;
  return couleur * d + vec3(s);
}`;

const VS_CARTE = `#version 300 es
in vec3 aPos; in vec3 aNormale;
uniform mat4 uMvp; uniform vec4 uCadre;
out vec2 vUv; out vec3 vN; out vec3 vP;
void main() {
  vUv = (aPos.xy - uCadre.xy) * uCadre.zw;
  vN = aNormale; vP = aPos;
  gl_Position = uMvp * vec4(aPos, 1.0);
}`;

const FS_CARTE = `#version 300 es
precision highp float;
in vec2 vUv; in vec3 vN; in vec3 vP;
uniform sampler2D uTex;
uniform int uMode;
uniform bool uVernis;
uniform vec3 uFinition, uVernisCouleur;
out vec4 sortie;
${ECLAIRAGE}
void main() {
  vec3 fr4 = vec3(0.70, 0.62, 0.40);
  if (uMode == 2) { sortie = vec4(eclairer(fr4 * 0.82, normalize(vN), vP, 0.0), 1.0); return; }
  vec4 t = texture(uTex, vUv);
  if (t.a > 0.5) discard;
  float cuivre = t.r, couvert = uVernis ? 1.0 - t.g : 0.0, serigraphie = t.b;
  vec3 nu = mix(fr4, uFinition, cuivre);
  vec3 sousVernis = mix(uVernisCouleur * 0.62, uVernisCouleur * 1.12 + vec3(0.03, 0.05, 0.0), cuivre);
  vec3 c = mix(nu, sousVernis, couvert);
  c = mix(c, vec3(0.93, 0.93, 0.90), serigraphie);
  float brillance = clamp(cuivre * (1.0 - couvert) * (1.0 - serigraphie) + 0.35 * couvert * (1.0 - serigraphie), 0.0, 1.0);
  sortie = vec4(eclairer(c, normalize(vN), vP, brillance), 1.0);
}`;

const VS_TROUS = `#version 300 es
in vec3 aUnite;
in vec4 iTrou;
uniform mat4 uMvp; uniform float uEpaisseur;
out vec3 vN; out vec3 vP; out float vMetal;
void main() {
  vec3 p = vec3(iTrou.xy + aUnite.xy * iTrou.z, aUnite.z * uEpaisseur);
  vN = vec3(-aUnite.xy, 0.0); vP = p; vMetal = iTrou.w;
  gl_Position = uMvp * vec4(p, 1.0);
}`;

const FS_TROUS = `#version 300 es
precision highp float;
in vec3 vN; in vec3 vP; in float vMetal;
uniform vec3 uFinition;
out vec4 sortie;
${ECLAIRAGE}
void main() {
  vec3 c = mix(vec3(0.58, 0.51, 0.33), uFinition * 0.9, vMetal);
  sortie = vec4(eclairer(c, normalize(vN), vP, vMetal * 0.8), 1.0);
}`;

const VS_BOITES = `#version 300 es
in vec3 aCoin; in vec3 aNormale;
in vec4 iPos; in vec4 iTaille; in vec3 iCouleur;
uniform mat4 uMvp; uniform int uChoisi;
out vec3 vN; out vec3 vP; out vec3 vC; out float vChoisi;
void main() {
  float c = cos(iPos.z), s = sin(iPos.z);
  vec2 l = aCoin.xy * iTaille.xy;
  vec3 p = vec3(iPos.x + l.x * c - l.y * s, iPos.y + l.x * s + l.y * c, iPos.w + aCoin.z * iTaille.z);
  vN = vec3(aNormale.x * c - aNormale.y * s, aNormale.x * s + aNormale.y * c, aNormale.z);
  vP = p; vC = iCouleur; vChoisi = gl_InstanceID == uChoisi ? 1.0 : 0.0;
  gl_Position = uMvp * vec4(p, 1.0);
}`;

const FS_BOITES = `#version 300 es
precision highp float;
in vec3 vN; in vec3 vP; in vec3 vC; in float vChoisi;
out vec4 sortie;
${ECLAIRAGE}
void main() {
  vec3 c = mix(vC, vec3(1.0, 0.62, 0.22), vChoisi * 0.75);
  sortie = vec4(eclairer(c, normalize(vN), vP, 0.25), 1.0);
}`;

function compiler(gl: WebGL2RenderingContext, vs: string, fs: string, attributs: string[]): WebGLProgram {
  const p = gl.createProgram()!;
  for (const [type, src] of [[gl.VERTEX_SHADER, vs], [gl.FRAGMENT_SHADER, fs]] as const) {
    const s = gl.createShader(type)!;
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(`Shader 3D : ${gl.getShaderInfoLog(s)}`);
    gl.attachShader(p, s);
  }
  attributs.forEach((a, i) => gl.bindAttribLocation(p, i, a));
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(`Programme 3D : ${gl.getProgramInfoLog(p)}`);
  return p;
}

/* --- Données de la scène ------------------------------------------------------------------------------------------- */

interface Boitier {
  composant: Composant;
  cx: number;
  cy: number;
  angle: number;
  sx: number;
  sy: number;
  z0: number;
  h: number;
  hauteurEstimee: boolean;
}

/** Couleur d'un boîtier selon la lettre de sa référence (R, C, U, J…). */
function couleurBoitier(ref: string): Vec3 {
  const l = ref.replace(/[^A-Za-z]/g, "").toUpperCase();
  if (l.startsWith("C")) return [0.62, 0.47, 0.30];
  if (l.startsWith("R")) return [0.10, 0.10, 0.11];
  if (l.startsWith("L") || l.startsWith("FB")) return [0.22, 0.22, 0.24];
  if (l.startsWith("D") || l.startsWith("LED")) return [0.16, 0.16, 0.17];
  if (/^(J|P|X|CN|CON|SW|S|K|BT)/.test(l)) return [0.86, 0.84, 0.78];
  if (l.startsWith("Y") || l.startsWith("X")) return [0.74, 0.75, 0.78];
  return [0.08, 0.08, 0.09];
}

const CUBE = (() => {
  // 6 faces × 2 triangles ; coin (x, y dans [−1, 1], z dans [0, 1]) et normale.
  const faces: [Vec3, Vec3, Vec3][] = [
    [[0, 0, 1], [1, 0, 0], [0, 1, 0]],
    [[0, 0, -1], [1, 0, 0], [0, -1, 0]],
    [[1, 0, 0], [0, 1, 0], [0, 0, 1]],
    [[-1, 0, 0], [0, -1, 0], [0, 0, 1]],
    [[0, 1, 0], [-1, 0, 0], [0, 0, 1]],
    [[0, -1, 0], [1, 0, 0], [0, 0, 1]],
  ];
  const v: number[] = [];
  for (const [n, u, w] of faces) {
    const coin = (a: number, b: number) => {
      const p = [n[0] + u[0] * a + w[0] * b, n[1] + u[1] * a + w[1] * b, n[2] + u[2] * a + w[2] * b];
      v.push(p[0], p[1], (p[2] + 1) / 2, ...n);
    };
    for (const [a, b] of [[-1, -1], [1, -1], [1, 1], [-1, -1], [1, 1], [-1, 1]]) coin(a, b);
  }
  return new Float32Array(v);
})();

/** Tube unitaire (rayon 1, hauteur 1), ouvert, en bande de triangles. */
const TUBE = (() => {
  const n = 24;
  const v: number[] = [];
  for (let i = 0; i <= n; i++) {
    const a = (i / n) * Math.PI * 2;
    v.push(Math.cos(a), Math.sin(a), 0, Math.cos(a), Math.sin(a), 1);
  }
  return new Float32Array(v);
})();

export class Vue3D {
  readonly racine: HTMLElement;
  private canvas: HTMLCanvasElement;
  private rendu: Rendu;
  private gl: WebGL2RenderingContext;
  private bulle: HTMLElement;
  private legende: HTMLElement;
  private pCarte: WebGLProgram;
  private pTrous: WebGLProgram;
  private pBoites: WebGLProgram;
  private uniformes = new Map<WebGLProgram, Map<string, WebGLUniformLocation | null>>();
  private pret = false;
  private detruite = false;
  private demande = 0;
  private cible: { largeur: number; hauteur: number; fbo: WebGLFramebuffer; resolu: WebGLFramebuffer; tampons: WebGLRenderbuffer[] } | null = null;

  // Scène
  private epaisseur = 1.6;
  private boite: [number, number, number, number] = [0, 0, 100, 100];
  private cadre: [number, number, number, number] = [0, 0, 0.01, 0.01];
  private textures: WebGLTexture[] = [];
  private vernis = [false, false];
  private vaoCarte: WebGLVertexArrayObject | null = null;
  private plages = { dessus: [0, 0], dessous: [0, 0], parois: [0, 0] };
  private vaoTrous: WebGLVertexArrayObject | null = null;
  private trous = 0;
  private vaoBoites: WebGLVertexArrayObject | null = null;
  private boitiers: Boitier[] = [];
  private tampons: WebGLBuffer[] = [];
  private finition: Vec3 = [0.86, 0.70, 0.36];
  montrerComposants = true;
  private choisi = -1;

  // Caméra (orbite autour d'une cible)
  private camera = { cible: [0, 0, 0] as Vec3, lacet: -Math.PI / 2, tangage: 0.95, distance: 100 };
  private pointeurs = new Map<number, { x: number; y: number }>();
  private glisse: { x: number; y: number; bouton: number; deplace: boolean; camera: Vue3D["camera"] } | null = null;
  private pince: { d: number; distance: number } | null = null;

  /** Appelé au clic sur un composant (ou sur le vide : null). */
  surChoix: (c: Composant | null) => void = () => {};

  constructor(private doc: DocumentTypon) {
    this.canvas = el("canvas", { class: "gl" });
    this.bulle = el("div", { class: "bulle-3d", hidden: true });
    this.legende = el("div", { class: "legende-3d" });
    this.racine = el("div", { class: "visionneuse vue-3d", tabindex: "0" }, this.canvas, this.bulle, this.legende);
    this.rendu = new Rendu(this.canvas);
    const gl = (this.gl = this.rendu.gl);
    this.pCarte = compiler(gl, VS_CARTE, FS_CARTE, ["aPos", "aNormale"]);
    this.pTrous = compiler(gl, VS_TROUS, FS_TROUS, ["aUnite", "iTrou"]);
    this.pBoites = compiler(gl, VS_BOITES, FS_BOITES, ["aCoin", "aNormale", "iPos", "iTaille", "iCouleur"]);
    new ResizeObserver(() => this.redimensionner()).observe(this.racine);
    this.installerEvenements();
  }

  private u(p: WebGLProgram, nom: string) {
    let m = this.uniformes.get(p);
    if (!m) this.uniformes.set(p, (m = new Map()));
    if (!m.has(nom)) m.set(nom, this.gl.getUniformLocation(p, nom));
    return m.get(nom)!;
  }

  /** Prépare la scène : contour, textures des deux faces, trous et boîtiers. */
  async charger(): Promise<void> {
    const gl = this.gl;
    const doc = this.doc;
    const meta = doc.description.meta;
    const modele = await moteur.modele3d(doc.lot);
    if (this.detruite) return;
    this.epaisseur = meta.epaisseur?.valeur && meta.epaisseur.valeur > 0 ? meta.epaisseur.valeur : 1.6;
    if (meta.finition && /hasl|tin|étain|sn|osp|silver|argent|immersion ag/i.test(meta.finition)) this.finition = /osp/i.test(meta.finition) ? [0.85, 0.52, 0.36] : [0.80, 0.81, 0.83];
    const e = this.epaisseur;

    // Boîte du bord et cadre des textures (marge d'un demi-millimètre).
    const bord = modele.anneaux[0] ?? [];
    let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
    for (const [x, y] of bord) {
      x0 = Math.min(x0, x);
      y0 = Math.min(y0, y);
      x1 = Math.max(x1, x);
      y1 = Math.max(y1, y);
    }
    if (!(x1 > x0 && y1 > y0)) throw new Error(t("3d.contourIntrouvable"));
    this.boite = [x0, y0, x1, y1];
    const m = 0.5;
    const cx0 = x0 - m, cy0 = y0 - m, cw = x1 - x0 + 2 * m, ch = y1 - y0 + 2 * m;
    const maxi = Math.min(4096, gl.getParameter(gl.MAX_TEXTURE_SIZE) as number);
    const echelle = Math.min(maxi / Math.max(cw, ch), 40);
    const largeur = Math.max(16, Math.round(cw * echelle));
    const hauteur = Math.max(16, Math.round(ch * echelle));
    this.cadre = [cx0, cy0, 1 / cw, 1 / ch];

    // Couches utiles : cuivres externes, vernis, sérigraphies, perçages et fraisages.
    const utiles = doc.couches.filter((c) => c.desc.objets > 0 && ["cuivre", "masque", "serigraphie", "percage", "fraisage"].includes(c.desc.role.genre) && (c.desc.role.genre === "percage" || c.desc.role.genre === "fraisage" || c.desc.role.cote === "dessus" || c.desc.role.cote === "dessous"));
    const geometries = new Map<number, { g: Geometrie; gpu: CoucheGpu }>();
    for (const c of utiles) {
      const g = await moteur.geometrie(doc.lot, c.desc.id);
      if (this.detruite) return;
      geometries.set(c.desc.id, { g, gpu: this.rendu.charger(g) });
    }
    const affichees = (filtre: (genre: string, cote: string) => boolean): CoucheAffichee[] =>
      utiles.filter((c) => filtre(c.desc.role.genre, c.desc.role.cote)).map((c) => ({ gpu: geometries.get(c.desc.id)!.gpu, couleur: [1, 1, 1], opacite: 1 }));
    const vue = { cx: cx0 + cw / 2, cy: cy0 + ch / 2, echelle: largeur / cw, miroir: false };
    const perces = affichees((g) => g === "percage" || g === "fraisage");
    for (const [i, cote] of (["dessus", "dessous"] as const).entries()) {
      const niveaux = Math.floor(Math.log2(Math.max(largeur, hauteur))) + 1;
      const t = gl.createTexture()!;
      gl.bindTexture(gl.TEXTURE_2D, t);
      gl.texStorage2D(gl.TEXTURE_2D, niveaux, gl.RGBA8, largeur, hauteur);
      const fbo = gl.createFramebuffer()!;
      gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, t, 0);
      gl.viewport(0, 0, largeur, hauteur);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
      const canaux: [CoucheAffichee[], number][] = [
        [affichees((g, c) => g === "cuivre" && c === cote), 0],
        [affichees((g, c) => g === "masque" && c === cote), 1],
        [affichees((g, c) => g === "serigraphie" && c === cote), 2],
        [perces, 3],
      ];
      this.vernis[i] = canaux[1][0].length > 0;
      for (const [couches, canal] of canaux) if (couches.length) this.rendu.peindreCanal(couches, vue, fbo, largeur, hauteur, canal);
      gl.deleteFramebuffer(fbo);
      gl.bindTexture(gl.TEXTURE_2D, t);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      const aniso = gl.getExtension("EXT_texture_filter_anisotropic");
      if (aniso) gl.texParameterf(gl.TEXTURE_2D, aniso.TEXTURE_MAX_ANISOTROPY_EXT, Math.min(8, gl.getParameter(aniso.MAX_TEXTURE_MAX_ANISOTROPY_EXT) as number));
      gl.generateMipmap(gl.TEXTURE_2D);
      this.textures.push(t);
    }
    this.rendu.libererCible();

    // Trous : cercles (segments de longueur nulle) des couches de perçage ; métallisés sauf mention contraire.
    const trous: number[] = [];
    for (const c of utiles.filter((c) => c.desc.role.genre === "percage" || c.desc.role.genre === "fraisage")) {
      const { g } = geometries.get(c.desc.id)!;
      const metal = /non.métallis|npth|non.plat/i.test(`${c.desc.role.libelle} ${c.desc.nom}`) ? 0 : 1;
      for (const lot of g.lots) {
        if (!lot.sombre) continue;
        for (let i = lot.caps[0]; i < lot.caps[0] + lot.caps[1]; i += 5) {
          const [ax, ay, bx, by, r] = [g.donnees[i], g.donnees[i + 1], g.donnees[i + 2], g.donnees[i + 3], g.donnees[i + 4]];
          if (r <= 0) continue;
          trous.push(ax, ay, r, metal);
          if (Math.hypot(bx - ax, by - ay) > 1e-4) trous.push(bx, by, r, metal);
        }
      }
    }
    for (const { gpu } of geometries.values()) this.rendu.liberer(gpu);

    // Plaque : faces du dessus et du dessous (triangles du moteur), parois le long de chaque anneau.
    const sommets: number[] = [];
    const tri = modele.triangles;
    for (const [z, nz] of [[e, 1], [0, -1]]) for (let i = 0; i < tri.length; i += 2) sommets.push(tri[i], tri[i + 1], z, 0, 0, nz);
    const nFace = tri.length / 2;
    for (const anneau of modele.anneaux) {
      for (let i = 0; i < anneau.length; i++) {
        const [ax, ay] = anneau[i];
        const [bx, by] = anneau[(i + 1) % anneau.length];
        const l = Math.hypot(bx - ax, by - ay) || 1;
        const [nx, ny] = [(by - ay) / l, -(bx - ax) / l];
        for (const [x, y, z] of [[ax, ay, 0], [bx, by, 0], [bx, by, e], [ax, ay, 0], [bx, by, e], [ax, ay, e]]) sommets.push(x, y, z, nx, ny, 0);
      }
    }
    this.plages = { dessus: [0, nFace], dessous: [nFace, nFace], parois: [2 * nFace, sommets.length / 6 - 2 * nFace] };
    this.vaoCarte = this.vao([[this.tampon(new Float32Array(sommets)), [[0, 3], [1, 3]], 6, 0]]);
    this.trous = trous.length / 4;
    this.vaoTrous = this.vao([[this.tampon(TUBE), [[0, 3]], 3, 0], [this.tampon(new Float32Array(trous.length ? trous : [0, 0, 0, 0])), [[1, 4]], 4, 1]]);

    // Composants : boîtiers d'après l'encombrement de leur empreinte.
    for (const c of meta.composants ?? []) {
      if (typeof c.x !== "number" || typeof c.y !== "number" || !c.corps) continue;
      // Trous de fixation, mires et points de test : pas de boîtier (sauf hauteur donnée par le fichier).
      if (!c.hauteur && /mounting.?hole|fiducial|testpoint|test.?pad|^(MH|H|FID|TP)\d/i.test(`${c.reference} ${c.boitier}`)) continue;
      const [bx0, by0, bx1, by1] = c.corps;
      const sx = (bx1 - bx0) / 2, sy = (by1 - by0) / 2;
      if (!(sx > 0.05 && sy > 0.05) || sx > 200 || sy > 200) continue;
      const dessous = /dessous|bot/i.test(c.cote);
      const a = ((typeof c.rotation === "number" ? c.rotation : parseFloat(c.rotation)) || 0) * (Math.PI / 180);
      // Repère du boîtier : miroir (dessous), rotation (sens trigonométrique), translation.
      let lx = (bx0 + bx1) / 2;
      const ly = (by0 + by1) / 2;
      if (dessous) lx = -lx;
      const cx = c.x + lx * Math.cos(a) - ly * Math.sin(a);
      const cy = c.y + lx * Math.sin(a) + ly * Math.cos(a);
      const estimee = !(c.hauteur && c.hauteur > 0);
      const h = estimee ? Math.min(3, Math.max(0.35, 0.3 * Math.min(sx, sy) * 2 + 0.25)) : c.hauteur!;
      this.boitiers.push({ composant: c, cx, cy, angle: a, sx, sy, z0: dessous ? 0 : e, h: dessous ? -h : h, hauteurEstimee: estimee });
    }
    const inst: number[] = [];
    for (const b of this.boitiers) inst.push(b.cx, b.cy, b.angle, b.z0, b.sx, b.sy, b.h, 0, ...couleurBoitier(b.composant.reference));
    this.vaoBoites = this.vao([[this.tampon(CUBE), [[0, 3], [1, 3]], 6, 0], [this.tampon(new Float32Array(inst.length ? inst : new Array(11).fill(0))), [[2, 4], [3, 4], [4, 3]], 11, 1]]);

    const source = meta.epaisseur ? t("3d.epaisseur", { e: nombre(e, 2), source: meta.epaisseur.source === "fichier de job" ? t("3d.sourceJob") : tm(meta.epaisseur.source) }) : t("3d.epaisseurDefaut");
    const sans = (meta.composants ?? []).length - this.boitiers.length;
    this.legende.replaceChildren(...[
      el("span", { text: source }),
      modele.source === "rectangle" ? el("span", { text: t("3d.rectangle") }) : null,
      this.boitiers.length ? el("span", { text: `${t("3d.boitiers", { n: this.boitiers.length })}${sans > 0 ? t("3d.nonRepresentes", { n: sans }) : ""}` }) : (meta.composants?.length ? el("span", { text: t("3d.sansEmpreinte") }) : null),
      el("span", { class: "aide", text: t("3d.aideCourte") }),
    ].filter((x): x is HTMLSpanElement => !!x));
    this.pret = true;
    this.recadrer();
  }

  private tampon(v: Float32Array): WebGLBuffer {
    const gl = this.gl;
    const t = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, t);
    gl.bufferData(gl.ARRAY_BUFFER, v, gl.STATIC_DRAW);
    this.tampons.push(t);
    return t;
  }

  /** Tableau de sommets : [tampon, [[attribut, taille]…], pas (flottants), diviseur]. */
  private vao(sources: [WebGLBuffer, [number, number][], number, number][]): WebGLVertexArrayObject {
    const gl = this.gl;
    const v = gl.createVertexArray()!;
    gl.bindVertexArray(v);
    for (const [t, attributs, pas, diviseur] of sources) {
      gl.bindBuffer(gl.ARRAY_BUFFER, t);
      let d = 0;
      for (const [loc, n] of attributs) {
        gl.enableVertexAttribArray(loc);
        gl.vertexAttribPointer(loc, n, gl.FLOAT, false, pas * 4, d * 4);
        gl.vertexAttribDivisor(loc, diviseur);
        d += n;
      }
    }
    gl.bindVertexArray(null);
    return v;
  }

  /** Vue d'ensemble, de trois quarts au-dessus (ou au-dessous). */
  recadrer(cote: "dessus" | "dessous" = this.camera.tangage < 0 ? "dessous" : "dessus") {
    const [x0, y0, x1, y1] = this.boite;
    // Distance pour que la sphère englobant la carte tienne dans le champ (vertical ou horizontal, le plus étroit).
    const rayon = Math.hypot(x1 - x0, y1 - y0) / 2;
    const r = this.racine.getBoundingClientRect();
    const aspect = r.width > 2 && r.height > 2 ? r.width / r.height : this.canvas.width / Math.max(1, this.canvas.height);
    const demi = (35 * Math.PI) / 360;
    const champ = Math.min(demi, Math.atan(Math.tan(demi) * aspect));
    this.racine.dataset.cote = cote;
    // Dessous : la carte est retournée autour de son axe vertical, comme on la retourne à la main (la sérigraphie
    // du dessous se lit à l'endroit).
    this.camera = { cible: [(x0 + x1) / 2, (y0 + y1) / 2, this.epaisseur / 2], lacet: cote === "dessous" ? Math.PI / 2 : -Math.PI / 2, tangage: cote === "dessous" ? -1.0 : 1.0, distance: (rayon / Math.sin(champ)) * 1.02 };
    this.redessiner();
  }

  /** Vue de face d'un côté (dessus ou dessous). */
  regarder(cote: "dessus" | "dessous") {
    this.recadrer(cote);
  }

  private redimensionner() {
    const r = this.racine.getBoundingClientRect();
    const ratio = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.round(r.width * ratio));
    const h = Math.max(1, Math.round(r.height * ratio));
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
    this.redessiner();
  }

  redessiner() {
    if (this.demande || this.detruite) return;
    this.demande = requestAnimationFrame(() => {
      this.demande = 0;
      this.dessiner();
    });
  }

  private matrices() {
    const { cible, lacet, tangage, distance } = this.camera;
    const oeil: Vec3 = [cible[0] + distance * Math.cos(tangage) * Math.cos(lacet), cible[1] + distance * Math.cos(tangage) * Math.sin(lacet), cible[2] + distance * Math.sin(tangage)];
    const taille = Math.hypot(this.boite[2] - this.boite[0], this.boite[3] - this.boite[1]);
    const proj = perspective((35 * Math.PI) / 180, this.canvas.width / Math.max(1, this.canvas.height), Math.max(0.01, distance * 0.02), distance + taille * 4);
    const vue = regarder(oeil, cible, [0, 0, 1]);
    return { oeil, proj, vue, mvp: multiplier(proj, vue) };
  }

  private preparerCible(largeur: number, hauteur: number) {
    const gl = this.gl;
    if (this.cible && this.cible.largeur === largeur && this.cible.hauteur === hauteur) return this.cible;
    if (this.cible) {
      gl.deleteFramebuffer(this.cible.fbo);
      gl.deleteFramebuffer(this.cible.resolu);
      for (const t of this.cible.tampons) gl.deleteRenderbuffer(t);
    }
    const echantillons = Math.min(4, gl.getParameter(gl.MAX_SAMPLES) as number);
    const fbo = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    const tampons = [[gl.RGBA8, gl.COLOR_ATTACHMENT0], [gl.DEPTH_COMPONENT24, gl.DEPTH_ATTACHMENT]].map(([format, attache]) => {
      const t = gl.createRenderbuffer()!;
      gl.bindRenderbuffer(gl.RENDERBUFFER, t);
      gl.renderbufferStorageMultisample(gl.RENDERBUFFER, echantillons, format, largeur, hauteur);
      gl.framebufferRenderbuffer(gl.FRAMEBUFFER, attache, gl.RENDERBUFFER, t);
      return t;
    });
    // Résolution dans un tampon simple de même format, puis copie vers le canevas (dont le format peut différer :
    // une copie depuis un tampon multi-échantillonné exige le même format).
    const resolu = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, resolu);
    const simple = gl.createRenderbuffer()!;
    gl.bindRenderbuffer(gl.RENDERBUFFER, simple);
    gl.renderbufferStorage(gl.RENDERBUFFER, gl.RGBA8, largeur, hauteur);
    gl.framebufferRenderbuffer(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.RENDERBUFFER, simple);
    tampons.push(simple);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    this.cible = { largeur, hauteur, fbo, resolu, tampons };
    return this.cible;
  }

  private dessiner() {
    const gl = this.gl;
    const W = this.canvas.width, H = this.canvas.height;
    const cible = this.preparerCible(W, H);
    // Fond sombre dans les deux thèmes, comme la vue 2D (blanc pour la fiche PDF).
    const fond = this.fondImage ?? [0.04, 0.055, 0.075];
    gl.bindFramebuffer(gl.FRAMEBUFFER, cible.fbo);
    gl.viewport(0, 0, W, H);
    gl.disable(gl.SCISSOR_TEST);
    gl.disable(gl.BLEND);
    gl.clearColor(fond[0], fond[1], fond[2], 1);
    gl.clearDepth(1);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    if (this.pret) {
      gl.enable(gl.DEPTH_TEST);
      gl.depthFunc(gl.LEQUAL);
      const { oeil, mvp } = this.matrices();
      // Plaque
      gl.useProgram(this.pCarte);
      gl.uniformMatrix4fv(this.u(this.pCarte, "uMvp"), false, mvp);
      gl.uniform3fv(this.u(this.pCarte, "uOeil"), oeil);
      gl.uniform4fv(this.u(this.pCarte, "uCadre"), this.cadre);
      gl.uniform3fv(this.u(this.pCarte, "uFinition"), this.finition);
      gl.uniform3fv(this.u(this.pCarte, "uVernisCouleur"), [0.07, 0.36, 0.17]);
      gl.uniform1i(this.u(this.pCarte, "uTex"), 0);
      gl.bindVertexArray(this.vaoCarte);
      for (const [mode, plage] of [[0, this.plages.dessus], [1, this.plages.dessous], [2, this.plages.parois]] as const) {
        if (!plage[1]) continue;
        gl.uniform1i(this.u(this.pCarte, "uMode"), mode);
        gl.uniform1i(this.u(this.pCarte, "uVernis"), mode < 2 && this.vernis[mode] ? 1 : 0);
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, this.textures[mode < 2 ? mode : 0]);
        gl.drawArrays(gl.TRIANGLES, plage[0], plage[1]);
      }
      // Parois des trous
      if (this.trous) {
        gl.useProgram(this.pTrous);
        gl.uniformMatrix4fv(this.u(this.pTrous, "uMvp"), false, mvp);
        gl.uniform3fv(this.u(this.pTrous, "uOeil"), oeil);
        gl.uniform1f(this.u(this.pTrous, "uEpaisseur"), this.epaisseur);
        gl.uniform3fv(this.u(this.pTrous, "uFinition"), this.finition);
        gl.bindVertexArray(this.vaoTrous);
        gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, TUBE.length / 3, this.trous);
      }
      // Boîtiers
      if (this.montrerComposants && this.boitiers.length) {
        gl.useProgram(this.pBoites);
        gl.uniformMatrix4fv(this.u(this.pBoites, "uMvp"), false, mvp);
        gl.uniform3fv(this.u(this.pBoites, "uOeil"), oeil);
        gl.uniform1i(this.u(this.pBoites, "uChoisi"), this.choisi);
        gl.bindVertexArray(this.vaoBoites);
        gl.drawArraysInstanced(gl.TRIANGLES, 0, 36, this.boitiers.length);
      }
      gl.bindVertexArray(null);
      gl.disable(gl.DEPTH_TEST);
    }
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, cible.fbo);
    gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER, cible.resolu);
    gl.blitFramebuffer(0, 0, W, H, 0, 0, W, H, gl.COLOR_BUFFER_BIT, gl.NEAREST);
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, cible.resolu);
    gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER, null);
    gl.blitFramebuffer(0, 0, W, H, 0, 0, W, H, gl.COLOR_BUFFER_BIT, gl.NEAREST);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  /* --- Interaction ------------------------------------------------------------------------------------------------- */

  /** Composant sous le pointeur (pixels CSS) : rayon de la caméra contre la boîte de chaque boîtier. */
  private composantSous(px: number, py: number): number {
    if (!this.montrerComposants || !this.boitiers.length) return -1;
    const r = this.racine.getBoundingClientRect();
    const { oeil, vue } = this.matrices();
    const fov = (35 * Math.PI) / 180;
    const t = Math.tan(fov / 2);
    const nx = ((px / r.width) * 2 - 1) * t * (r.width / r.height);
    const ny = (1 - (py / r.height) * 2) * t;
    // Lignes de la matrice de vue = axes de la caméra.
    const ax: Vec3 = [vue[0], vue[4], vue[8]], ay: Vec3 = [vue[1], vue[5], vue[9]], az: Vec3 = [vue[2], vue[6], vue[10]];
    const d = norm([ax[0] * nx + ay[0] * ny - az[0], ax[1] * nx + ay[1] * ny - az[1], ax[2] * nx + ay[2] * ny - az[2]]);
    let meilleur = -1;
    let tMin = Infinity;
    this.boitiers.forEach((b, i) => {
      const c = Math.cos(-b.angle), s = Math.sin(-b.angle);
      const ox = oeil[0] - b.cx, oy = oeil[1] - b.cy;
      const o: Vec3 = [ox * c - oy * s, ox * s + oy * c, oeil[2]];
      const v: Vec3 = [d[0] * c - d[1] * s, d[0] * s + d[1] * c, d[2]];
      const mini: Vec3 = [-b.sx, -b.sy, Math.min(b.z0, b.z0 + b.h)];
      const maxi: Vec3 = [b.sx, b.sy, Math.max(b.z0, b.z0 + b.h)];
      let t0 = -Infinity, t1 = Infinity;
      for (let k = 0; k < 3; k++) {
        if (Math.abs(v[k]) < 1e-12) {
          if (o[k] < mini[k] || o[k] > maxi[k]) return;
          continue;
        }
        let a = (mini[k] - o[k]) / v[k], bb = (maxi[k] - o[k]) / v[k];
        if (a > bb) [a, bb] = [bb, a];
        t0 = Math.max(t0, a);
        t1 = Math.min(t1, bb);
      }
      if (t0 <= t1 && t1 > 0 && t0 < tMin) {
        tMin = t0;
        meilleur = i;
      }
    });
    return meilleur;
  }

  private choisir(i: number, px = 0, py = 0) {
    this.choisi = i;
    const b = this.boitiers[i];
    if (!b) {
      this.bulle.hidden = true;
      this.surChoix(null);
    } else {
      const c = b.composant;
      this.bulle.replaceChildren(
        el("strong", { text: c.reference }),
        el("span", { text: [c.valeur, c.boitier].filter(Boolean).join(" · ") }),
        el("span", { text: `${/dessous|bot/i.test(c.cote) ? t("3d.dessous") : t("3d.dessus")} · ${nombre(b.sx * 2, 2)} × ${nombre(b.sy * 2, 2)} × ${nombre(Math.abs(b.h), 2)} mm${b.hauteurEstimee ? t("3d.hauteurEstimee") : ""}` }),
      );
      this.bulle.style.left = `${px + 14}px`;
      this.bulle.style.top = `${py + 14}px`;
      this.bulle.hidden = false;
      this.surChoix(c);
    }
    this.redessiner();
  }

  private installerEvenements() {
    const c = this.racine;
    const local = (e: { clientX: number; clientY: number }) => {
      const r = c.getBoundingClientRect();
      return { x: e.clientX - r.left, y: e.clientY - r.top };
    };
    c.addEventListener("contextmenu", (e) => e.preventDefault());
    c.addEventListener("wheel", (e) => {
      e.preventDefault();
      const d = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
      const pincement = e.ctrlKey && Math.abs(d) < 40;
      this.zoomer(Math.exp(d * (pincement ? 0.01 : 0.0018)));
    }, { passive: false });
    c.addEventListener("pointerdown", (e) => {
      c.setPointerCapture(e.pointerId);
      const p = local(e);
      this.pointeurs.set(e.pointerId, p);
      if (this.pointeurs.size === 1) {
        const bouton = e.button === 2 || e.button === 1 || e.shiftKey || e.ctrlKey ? 2 : 0;
        this.glisse = { ...p, bouton, deplace: false, camera: { ...this.camera, cible: [...this.camera.cible] as Vec3 } };
      } else if (this.pointeurs.size === 2) {
        const [a, b] = [...this.pointeurs.values()];
        this.pince = { d: Math.hypot(a.x - b.x, a.y - b.y), distance: this.camera.distance };
        this.glisse = null;
      }
    });
    c.addEventListener("pointermove", (e) => {
      const p = local(e);
      if (this.pointeurs.has(e.pointerId)) this.pointeurs.set(e.pointerId, p);
      if (this.pince && this.pointeurs.size === 2) {
        const [a, b] = [...this.pointeurs.values()];
        const d = Math.hypot(a.x - b.x, a.y - b.y);
        this.camera.distance = this.limiterDistance(this.pince.distance * (this.pince.d / Math.max(1, d)));
        this.redessiner();
        return;
      }
      const g = this.glisse;
      if (!g) return;
      const dx = p.x - g.x, dy = p.y - g.y;
      if (Math.hypot(dx, dy) > 4) g.deplace = true;
      if (!g.deplace) return;
      if (g.bouton === 0) {
        this.camera.lacet = g.camera.lacet - dx * 0.008;
        this.camera.tangage = Math.max(-1.55, Math.min(1.55, g.camera.tangage + dy * 0.008));
      } else {
        // Déplacement dans le plan de l'écran, à la vitesse du pointeur au niveau de la cible.
        const h = this.racine.getBoundingClientRect().height || 1;
        const k = (2 * g.camera.distance * Math.tan((35 * Math.PI) / 360)) / h;
        const { vue } = this.matrices();
        const ax: Vec3 = [vue[0], vue[4], vue[8]], ay: Vec3 = [vue[1], vue[5], vue[9]];
        this.camera.cible = [0, 1, 2].map((i) => g.camera.cible[i] - (ax[i] * dx - ay[i] * dy) * k) as Vec3;
      }
      this.redessiner();
    });
    const fin = (e: PointerEvent) => {
      const p = local(e);
      const etaitPince = this.pointeurs.size === 2;
      this.pointeurs.delete(e.pointerId);
      if (this.pointeurs.size < 2) this.pince = null;
      if (this.glisse && !this.glisse.deplace && !etaitPince && e.type === "pointerup") this.choisir(this.composantSous(p.x, p.y), p.x, p.y);
      this.glisse = null;
    };
    c.addEventListener("pointerup", fin);
    c.addEventListener("pointercancel", fin);
    c.addEventListener("keydown", (e) => {
      if (e.key === "+" || e.key === "=") this.zoomer(1 / 1.3);
      else if (e.key === "-") this.zoomer(1.3);
      else if (e.key === "ArrowLeft") this.camera.lacet += 0.12;
      else if (e.key === "ArrowRight") this.camera.lacet -= 0.12;
      else if (e.key === "ArrowUp") this.camera.tangage = Math.min(1.55, this.camera.tangage + 0.12);
      else if (e.key === "ArrowDown") this.camera.tangage = Math.max(-1.55, this.camera.tangage - 0.12);
      else return;
      e.preventDefault();
      this.redessiner();
    });
  }

  private limiterDistance(d: number) {
    const taille = Math.hypot(this.boite[2] - this.boite[0], this.boite[3] - this.boite[1]);
    return Math.max(taille * 0.02, Math.min(taille * 6, d));
  }

  zoomer(facteur: number) {
    this.camera.distance = this.limiterDistance(this.camera.distance * facteur);
    this.redessiner();
  }

  /** Capture de la vue (PNG). */
  image(): string {
    this.dessiner();
    return this.canvas.toDataURL("image/png");
  }

  /** Fond imposé (capture hors écran pour la fiche PDF) ; null : fond de l'écran. */
  fondImage: number[] | null = null;

  /**
   * Image PNG de la carte en trois quarts, d'un côté, à la taille demandée : pour une vue qui n'est pas affichée
   * (fiche PDF). La scène doit être chargée (`charger`).
   */
  async capture(largeur: number, hauteur: number, cote: "dessus" | "dessous" = "dessus"): Promise<Uint8Array> {
    this.canvas.width = largeur;
    this.canvas.height = hauteur;
    this.recadrer(cote);
    cancelAnimationFrame(this.demande);
    this.demande = 0;
    this.dessiner();
    const blob = await new Promise<Blob>((ok, ko) => this.canvas.toBlob((b) => (b ? ok(b) : ko(new Error("PNG"))), "image/png"));
    return new Uint8Array(await blob.arrayBuffer());
  }

  detruire() {
    this.detruite = true;
    cancelAnimationFrame(this.demande);
    const gl = this.gl;
    for (const t of this.textures) gl.deleteTexture(t);
    for (const t of this.tampons) gl.deleteBuffer(t);
    for (const v of [this.vaoCarte, this.vaoTrous, this.vaoBoites]) if (v) gl.deleteVertexArray(v);
    if (this.cible) {
      gl.deleteFramebuffer(this.cible.fbo);
      gl.deleteFramebuffer(this.cible.resolu);
      for (const t of this.cible.tampons) gl.deleteRenderbuffer(t);
    }
    gl.getExtension("WEBGL_lose_context")?.loseContext();
    this.racine.remove();
  }
}
