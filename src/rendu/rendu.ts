/**
 * Affichage WebGL2 des couches. Chaque couche est d'abord peinte dans une texture (masque de couverture, lots de
 * polarité sombre puis claire dans l'ordre du fichier), puis composée à l'écran avec sa couleur et son opacité. Les
 * segments à bouts ronds et les rectangles arrondis sont dessinés par instances avec un champ de distance (bords lissés,
 * trait fin d'au moins un pixel) ; les polygones sont triangulés par le moteur et lissés par multi-échantillonnage.
 */
import type { Geometrie } from "../moteur/client";

export interface Vue {
  /** Centre de l'écran, en mm. */
  cx: number;
  cy: number;
  /** Pixels (de l'appareil) par mm. */
  echelle: number;
  /** Vue de dessous : miroir horizontal. */
  miroir: boolean;
}

export interface CoucheGpu {
  lots: { sombre: boolean; caps: [number, number]; boites: [number, number]; tris: [number, number] }[];
  vaoCaps: WebGLVertexArrayObject;
  vaoBoites: WebGLVertexArrayObject;
  vaoTris: WebGLVertexArrayObject;
  tampons: WebGLBuffer[];
  boite: [number, number, number, number];
}

export interface CoucheAffichee {
  gpu: CoucheGpu;
  couleur: [number, number, number];
  opacite: number;
  /** Décalage appliqué (alignement d'une comparaison), en mm. */
  dx?: number;
  dy?: number;
}

export interface Scene {
  /** Rectangle de l'écran (pixels de l'appareil, origine en bas à gauche, comme WebGL). */
  x: number;
  y: number;
  w: number;
  h: number;
  vue: Vue;
  couches: CoucheAffichee[];
  /** Superposition de comparaison : A en rouge (disparu), B en vert (apparu), commun en gris. */
  difference?: { a: CoucheAffichee[]; b: CoucheAffichee[] };
}

const VS_CAPS = `#version 300 es
in vec2 aCoin;
in vec4 iAB;
in float iR;
uniform vec4 uVue;
uniform float uPx;
uniform vec2 uDecalage;
out vec2 vP; out vec2 vA; out vec2 vB; out float vR;
void main() {
  vec2 a = iAB.xy + uDecalage, b = iAB.zw + uDecalage;
  float r = max(iR, 0.55 * uPx);
  vec2 d = b - a; float l = length(d);
  vec2 u = l > 0.0 ? d / l : vec2(1.0, 0.0);
  vec2 n = vec2(-u.y, u.x);
  float e = r + 1.5 * uPx;
  vec2 p = (a + b) * 0.5 + u * aCoin.x * (l * 0.5 + e) + n * aCoin.y * e;
  vP = p; vA = a; vB = b; vR = r;
  gl_Position = vec4(p * uVue.xy + uVue.zw, 0.0, 1.0);
}`;

const FS_CAPS = `#version 300 es
precision highp float;
in vec2 vP; in vec2 vA; in vec2 vB; in float vR;
uniform float uPx;
out vec4 sortie;
void main() {
  vec2 pa = vP - vA, ba = vB - vA;
  float h = dot(ba, ba) > 0.0 ? clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0) : 0.0;
  float d = length(pa - ba * h) - vR;
  float c = clamp(0.5 - d / uPx, 0.0, 1.0);
  if (c <= 0.0) discard;
  sortie = vec4(c);
}`;

const VS_BOITES = `#version 300 es
in vec2 aCoin;
in vec2 iC; in vec2 iS; in vec2 iA;
uniform vec4 uVue;
uniform float uPx;
uniform vec2 uDecalage;
out vec2 vL; out vec2 vS; out float vR;
void main() {
  float e = 1.5 * uPx;
  vec2 s = max(iS, vec2(0.5 * uPx));
  vec2 l = aCoin * (s + e);
  float c = cos(iA.x), sn = sin(iA.x);
  vec2 p = iC + uDecalage + vec2(l.x * c - l.y * sn, l.x * sn + l.y * c);
  vL = l; vS = s; vR = min(iA.y, min(s.x, s.y));
  gl_Position = vec4(p * uVue.xy + uVue.zw, 0.0, 1.0);
}`;

const FS_BOITES = `#version 300 es
precision highp float;
in vec2 vL; in vec2 vS; in float vR;
uniform float uPx;
out vec4 sortie;
void main() {
  vec2 q = abs(vL) - (vS - vec2(vR));
  float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - vR;
  float c = clamp(0.5 - d / uPx, 0.0, 1.0);
  if (c <= 0.0) discard;
  sortie = vec4(c);
}`;

const VS_TRIS = `#version 300 es
in vec2 aPos;
uniform vec4 uVue;
uniform vec2 uDecalage;
void main() { gl_Position = vec4((aPos + uDecalage) * uVue.xy + uVue.zw, 0.0, 1.0); }`;

const FS_TRIS = `#version 300 es
precision highp float;
out vec4 sortie;
void main() { sortie = vec4(1.0); }`;

const VS_PLEIN = `#version 300 es
out vec2 vUv;
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  vUv = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

const FS_COMPOSER = `#version 300 es
precision highp float;
in vec2 vUv;
uniform sampler2D uMasque;
uniform vec3 uCouleur;
uniform float uOpacite;
out vec4 sortie;
void main() {
  float c = texture(uMasque, vUv).r * uOpacite;
  sortie = vec4(uCouleur * c, c);
}`;

const FS_DIFFERENCE = `#version 300 es
precision highp float;
in vec2 vUv;
uniform sampler2D uA;
uniform sampler2D uB;
out vec4 sortie;
void main() {
  float a = texture(uA, vUv).r, b = texture(uB, vUv).r;
  float commun = min(a, b), seulA = max(a - b, 0.0), seulB = max(b - a, 0.0);
  vec3 c = vec3(0.50, 0.55, 0.62) * commun * 0.55 + vec3(1.0, 0.27, 0.27) * seulA + vec3(0.25, 0.92, 0.45) * seulB;
  float alpha = clamp(commun * 0.55 + seulA + seulB, 0.0, 1.0);
  sortie = vec4(c, alpha);
}`;

function compiler(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const p = gl.createProgram()!;
  for (const [type, src] of [[gl.VERTEX_SHADER, vs], [gl.FRAGMENT_SHADER, fs]] as const) {
    const s = gl.createShader(type)!;
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(`Shader : ${gl.getShaderInfoLog(s)}`);
    gl.attachShader(p, s);
  }
  gl.bindAttribLocation(p, 0, "aCoin");
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(`Programme : ${gl.getProgramInfoLog(p)}`);
  return p;
}

/** Cibles de peinture : deux tampons multi-échantillonnés, chacun résolu dans sa texture (un seul tampon partagé ne
 * suffit pas : certains pilotes diffèrent la résolution, et la peinture suivante effaçait la texture précédente). */
interface Cible {
  largeur: number;
  hauteur: number;
  msaa: WebGLFramebuffer[];
  tampons: WebGLRenderbuffer[];
  resolu: WebGLFramebuffer[];
  textures: WebGLTexture[];
}

export class Rendu {
  readonly gl: WebGL2RenderingContext;
  private pCaps: WebGLProgram;
  private pBoites: WebGLProgram;
  private pTris: WebGLProgram;
  private pComposer: WebGLProgram;
  private pDifference: WebGLProgram;
  private coin: WebGLBuffer;
  private cible: Cible | null = null;
  private echantillons: number;
  private uniformes = new Map<WebGLProgram, Map<string, WebGLUniformLocation | null>>();

  constructor(readonly canvas: HTMLCanvasElement) {
    const gl = canvas.getContext("webgl2", { antialias: false, alpha: false, premultipliedAlpha: true, preserveDrawingBuffer: true });
    if (!gl) throw new Error("Ce navigateur ne sait pas afficher en WebGL 2 : mettez-le à jour, ou essayez Chrome, Edge, Firefox ou Safari récents.");
    this.gl = gl;
    this.pCaps = compiler(gl, VS_CAPS, FS_CAPS);
    this.pBoites = compiler(gl, VS_BOITES, FS_BOITES);
    this.pTris = compiler(gl, VS_TRIS, FS_TRIS);
    this.pComposer = compiler(gl, VS_PLEIN, FS_COMPOSER);
    this.pDifference = compiler(gl, VS_PLEIN, FS_DIFFERENCE);
    this.coin = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.coin);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    this.echantillons = Math.min(4, gl.getParameter(gl.MAX_SAMPLES) as number);
  }

  private u(p: WebGLProgram, nom: string): WebGLUniformLocation | null {
    let m = this.uniformes.get(p);
    if (!m) this.uniformes.set(p, (m = new Map()));
    if (!m.has(nom)) m.set(nom, this.gl.getUniformLocation(p, nom));
    return m.get(nom)!;
  }

  /** Envoie la géométrie d'une couche à la carte graphique. */
  charger(g: Geometrie): CoucheGpu {
    const gl = this.gl;
    const d = g.donnees;
    // Les lots occupent des plages contiguës par genre ? Non : ils alternent. On crée un tampon par genre en
    // concaténant les plages, et on retient la position de chaque lot dans ces tampons.
    const caps: number[] = [];
    const boites: number[] = [];
    const tris: number[] = [];
    const lots = g.lots.map((l) => {
      const c: [number, number] = [caps.length / 5, l.caps[1] / 5];
      for (let i = 0; i < l.caps[1]; i++) caps.push(d[l.caps[0] + i]);
      const b: [number, number] = [boites.length / 6, l.boites[1] / 6];
      for (let i = 0; i < l.boites[1]; i++) boites.push(d[l.boites[0] + i]);
      const t: [number, number] = [tris.length / 2, l.tris[1] / 2];
      for (let i = 0; i < l.tris[1]; i++) tris.push(d[l.tris[0] + i]);
      return { sombre: l.sombre, caps: c, boites: b, tris: t };
    });
    const tampons: WebGLBuffer[] = [];
    const tampon = (v: number[]) => {
      const t = gl.createBuffer()!;
      gl.bindBuffer(gl.ARRAY_BUFFER, t);
      gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(v), gl.STATIC_DRAW);
      tampons.push(t);
      return t;
    };
    const instancie = (prog: WebGLProgram, t: WebGLBuffer, attributs: [string, number][], pas: number) => {
      const vao = gl.createVertexArray()!;
      gl.bindVertexArray(vao);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.coin);
      gl.enableVertexAttribArray(0);
      gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
      gl.bindBuffer(gl.ARRAY_BUFFER, t);
      let decalage = 0;
      for (const [nom, n] of attributs) {
        const loc = gl.getAttribLocation(prog, nom);
        if (loc >= 0) {
          gl.enableVertexAttribArray(loc);
          gl.vertexAttribPointer(loc, n, gl.FLOAT, false, pas * 4, decalage * 4);
          gl.vertexAttribDivisor(loc, 1);
        }
        decalage += n;
      }
      gl.bindVertexArray(null);
      return vao;
    };
    const vaoCaps = instancie(this.pCaps, tampon(caps), [["iAB", 4], ["iR", 1]], 5);
    const vaoBoites = instancie(this.pBoites, tampon(boites), [["iC", 2], ["iS", 2], ["iA", 2]], 6);
    const vaoTris = gl.createVertexArray()!;
    gl.bindVertexArray(vaoTris);
    gl.bindBuffer(gl.ARRAY_BUFFER, tampon(tris));
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.bindVertexArray(null);
    return { lots, vaoCaps, vaoBoites, vaoTris, tampons, boite: g.boite };
  }

  liberer(c: CoucheGpu) {
    const gl = this.gl;
    for (const t of c.tampons) gl.deleteBuffer(t);
    gl.deleteVertexArray(c.vaoCaps);
    gl.deleteVertexArray(c.vaoBoites);
    gl.deleteVertexArray(c.vaoTris);
  }

  private preparerCible(largeur: number, hauteur: number): Cible {
    const gl = this.gl;
    if (this.cible && this.cible.largeur === largeur && this.cible.hauteur === hauteur) return this.cible;
    if (this.cible) this.detruire(this.cible);
    const msaa: WebGLFramebuffer[] = [];
    const tampons: WebGLRenderbuffer[] = [];
    const resolu: WebGLFramebuffer[] = [];
    const textures: WebGLTexture[] = [];
    for (let i = 0; i < 2; i++) {
      const tampon = gl.createRenderbuffer()!;
      gl.bindRenderbuffer(gl.RENDERBUFFER, tampon);
      gl.renderbufferStorageMultisample(gl.RENDERBUFFER, this.echantillons, gl.RGBA8, largeur, hauteur);
      const m = gl.createFramebuffer()!;
      gl.bindFramebuffer(gl.FRAMEBUFFER, m);
      gl.framebufferRenderbuffer(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.RENDERBUFFER, tampon);
      msaa.push(m);
      tampons.push(tampon);
      const t = gl.createTexture()!;
      gl.bindTexture(gl.TEXTURE_2D, t);
      gl.texStorage2D(gl.TEXTURE_2D, 1, gl.RGBA8, largeur, hauteur);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
      const f = gl.createFramebuffer()!;
      gl.bindFramebuffer(gl.FRAMEBUFFER, f);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, t, 0);
      resolu.push(f);
      textures.push(t);
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    this.cible = { largeur, hauteur, msaa, tampons, resolu, textures };
    return this.cible;
  }

  private detruire(c: Cible) {
    const gl = this.gl;
    for (const f of [...c.msaa, ...c.resolu]) gl.deleteFramebuffer(f);
    for (const t of c.tampons) gl.deleteRenderbuffer(t);
    for (const t of c.textures) gl.deleteTexture(t);
  }

  /** Uniformes de vue pour un rectangle d'écran (clip = p·s + o). */
  private parametresVue(s: { x: number; y: number; w: number; h: number; vue: Vue }) {
    const { vue } = s;
    const sx = ((vue.miroir ? -1 : 1) * 2 * vue.echelle) / s.w;
    const sy = (2 * vue.echelle) / s.h;
    return { vue: [sx, sy, -vue.cx * sx, -vue.cy * sy] as [number, number, number, number], px: 1 / vue.echelle };
  }

  /** Peint le masque d'une ou plusieurs couches (unies) dans la texture `index` de la cible. */
  private peindre(cible: Cible, scene: Scene, couches: CoucheAffichee[], index: number) {
    const gl = this.gl;
    gl.bindFramebuffer(gl.FRAMEBUFFER, cible.msaa[index]);
    gl.viewport(scene.x, scene.y, scene.w, scene.h);
    gl.enable(gl.SCISSOR_TEST);
    gl.scissor(scene.x, scene.y, scene.w, scene.h);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.enable(gl.BLEND);
    const { vue, px } = this.parametresVue(scene);
    for (const c of couches) {
      for (const lot of c.gpu.lots) {
        if (lot.sombre) gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
        else gl.blendFunc(gl.ZERO, gl.ONE_MINUS_SRC_ALPHA);
        for (const [prog, vao, plage, mode] of [
          [this.pTris, c.gpu.vaoTris, lot.tris, "tris"],
          [this.pBoites, c.gpu.vaoBoites, lot.boites, "inst"],
          [this.pCaps, c.gpu.vaoCaps, lot.caps, "inst"],
        ] as const) {
          if (plage[1] === 0) continue;
          gl.useProgram(prog);
          gl.uniform4fv(this.u(prog, "uVue"), vue);
          gl.uniform1f(this.u(prog, "uPx"), px);
          gl.uniform2f(this.u(prog, "uDecalage"), c.dx ?? 0, c.dy ?? 0);
          gl.bindVertexArray(vao);
          if (mode === "tris") {
            gl.drawArrays(gl.TRIANGLES, plage[0], plage[1]);
          } else {
            // Les attributs d'instance commencent au début du tampon : on décale par baseInstance simulé.
            this.attribuerDebut(prog, vao, plage[0]);
            gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, plage[1]);
          }
        }
      }
    }
    gl.bindVertexArray(null);
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, cible.msaa[index]);
    gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER, cible.resolu[index]);
    gl.blitFramebuffer(scene.x, scene.y, scene.x + scene.w, scene.y + scene.h, scene.x, scene.y, scene.x + scene.w, scene.y + scene.h, gl.COLOR_BUFFER_BIT, gl.NEAREST);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  /** WebGL 2 n'a pas de « base instance » : on redéfinit le début des attributs d'instance pour ce lot. */
  private attribuerDebut(prog: WebGLProgram, vao: WebGLVertexArrayObject, debut: number) {
    const gl = this.gl;
    const champs: [string, number][] = prog === this.pCaps ? [["iAB", 4], ["iR", 1]] : [["iC", 2], ["iS", 2], ["iA", 2]];
    const pas = prog === this.pCaps ? 5 : 6;
    gl.bindVertexArray(vao);
    const t = gl.getVertexAttrib(gl.getAttribLocation(prog, champs[0][0]), gl.VERTEX_ATTRIB_ARRAY_BUFFER_BINDING) as WebGLBuffer;
    gl.bindBuffer(gl.ARRAY_BUFFER, t);
    let d = 0;
    for (const [nom, n] of champs) {
      const loc = gl.getAttribLocation(prog, nom);
      if (loc >= 0) gl.vertexAttribPointer(loc, n, gl.FLOAT, false, pas * 4, (debut * pas + d) * 4);
      d += n;
    }
  }

  /** Dessine les scènes (une seule, ou deux côte à côte) sur le canevas. */
  dessiner(scenes: Scene[], fond: [number, number, number]) {
    const gl = this.gl;
    const cible = this.preparerCible(this.canvas.width, this.canvas.height);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.disable(gl.SCISSOR_TEST);
    gl.viewport(0, 0, this.canvas.width, this.canvas.height);
    gl.clearColor(fond[0], fond[1], fond[2], 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    for (const scene of scenes) {
      if (scene.difference) {
        this.peindre(cible, scene, scene.difference.a, 0);
        this.peindre(cible, scene, scene.difference.b, 1);
        this.composerDifference(cible, scene);
        continue;
      }
      for (const c of scene.couches) {
        this.peindre(cible, scene, [c], 0);
        this.composer(cible, scene, c);
      }
    }
    gl.disable(gl.SCISSOR_TEST);
  }

  private composer(cible: Cible, scene: Scene, c: CoucheAffichee) {
    const gl = this.gl;
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, cible.largeur, cible.hauteur);
    gl.enable(gl.SCISSOR_TEST);
    gl.scissor(scene.x, scene.y, scene.w, scene.h);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    gl.useProgram(this.pComposer);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, cible.textures[0]);
    gl.uniform1i(this.u(this.pComposer, "uMasque"), 0);
    gl.uniform3fv(this.u(this.pComposer, "uCouleur"), c.couleur);
    gl.uniform1f(this.u(this.pComposer, "uOpacite"), c.opacite);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  private composerDifference(cible: Cible, scene: Scene) {
    const gl = this.gl;
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, cible.largeur, cible.hauteur);
    gl.enable(gl.SCISSOR_TEST);
    gl.scissor(scene.x, scene.y, scene.w, scene.h);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    gl.useProgram(this.pDifference);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, cible.textures[0]);
    gl.uniform1i(this.u(this.pDifference, "uA"), 0);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, cible.textures[1]);
    gl.uniform1i(this.u(this.pDifference, "uB"), 1);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.activeTexture(gl.TEXTURE0);
  }

  /**
   * Peint des couches (unies) dans un canal d'une texture RGBA déjà allouée (vue 3D : un canal par genre de couche).
   * La vue cadre la texture ; la texture doit avoir la taille `largeur` × `hauteur`.
   */
  peindreCanal(couches: CoucheAffichee[], vue: Vue, cadre: WebGLFramebuffer, largeur: number, hauteur: number, canal: number) {
    const gl = this.gl;
    const cible = this.preparerCible(largeur, hauteur);
    this.peindre(cible, { x: 0, y: 0, w: largeur, h: hauteur, vue, couches: [] }, couches, 0);
    gl.bindFramebuffer(gl.FRAMEBUFFER, cadre);
    gl.viewport(0, 0, largeur, hauteur);
    gl.disable(gl.SCISSOR_TEST);
    gl.disable(gl.BLEND);
    gl.disable(gl.DEPTH_TEST);
    gl.colorMask(canal === 0, canal === 1, canal === 2, canal === 3);
    gl.useProgram(this.pComposer);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, cible.textures[0]);
    gl.uniform1i(this.u(this.pComposer, "uMasque"), 0);
    gl.uniform3fv(this.u(this.pComposer, "uCouleur"), [1, 1, 1]);
    gl.uniform1f(this.u(this.pComposer, "uOpacite"), 1);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.colorMask(true, true, true, true);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  /** Libère les tampons de peinture (après la préparation des textures de la vue 3D). */
  libererCible() {
    if (this.cible) this.detruire(this.cible);
    this.cible = null;
  }

  /**
   * Masques de deux jeux de couches sur une même grille (pour chercher les différences) : rendu hors écran, lu en
   * mémoire. Renvoie un octet par pixel : bit 0 = présent dans A, bit 1 = présent dans B. Ligne 0 = haut.
   */
  masques(a: CoucheAffichee[], b: CoucheAffichee[], vue: Vue, largeur: number, hauteur: number): Uint8Array {
    const gl = this.gl;
    const sauvegarde = this.cible;
    this.cible = null;
    const cible = this.preparerCible(largeur, hauteur);
    const scene: Scene = { x: 0, y: 0, w: largeur, h: hauteur, vue, couches: [] };
    const lire = (couches: CoucheAffichee[], index: number) => {
      this.peindre(cible, scene, couches, index);
      const px = new Uint8Array(largeur * hauteur * 4);
      gl.bindFramebuffer(gl.FRAMEBUFFER, cible.resolu[index]);
      gl.readPixels(0, 0, largeur, hauteur, gl.RGBA, gl.UNSIGNED_BYTE, px);
      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
      return px;
    };
    const pa = lire(a, 0);
    const pb = lire(b, 1);
    const m = new Uint8Array(largeur * hauteur);
    for (let y = 0; y < hauteur; y++) {
      const ligne = (hauteur - 1 - y) * largeur;
      for (let x = 0; x < largeur; x++) {
        const i = (y * largeur + x) * 4;
        m[ligne + x] = (pa[i] > 127 ? 1 : 0) | (pb[i] > 127 ? 2 : 0);
      }
    }
    // Libère la cible d'analyse et retrouve celle de l'écran.
    this.detruire(cible);
    this.cible = sauvegarde;
    return m;
  }
}

export function couleurRgb(hex: string): [number, number, number] {
  const n = parseInt(hex.replace("#", ""), 16);
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}
