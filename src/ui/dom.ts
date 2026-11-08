/** Petits utilitaires d'interface. */

type Attributs = Record<string, unknown> & { class?: string; text?: string; html?: string; style?: Partial<CSSStyleDeclaration> };

export function el<K extends keyof HTMLElementTagNameMap>(tag: K, attrs: Attributs = {}, ...enfants: (Node | string | null | undefined | false)[]): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === undefined || v === null || v === false) continue;
    if (k === "class") e.className = String(v);
    else if (k === "text") e.textContent = String(v);
    else if (k === "html") e.innerHTML = String(v);
    else if (k === "style") Object.assign(e.style, v);
    else if (k.startsWith("on") && typeof v === "function") e.addEventListener(k.slice(2), v as EventListener);
    else if (v === true) e.setAttribute(k, "");
    else e.setAttribute(k, String(v));
  }
  for (const c of enfants) if (c !== null && c !== undefined && c !== false) e.append(c);
  return e;
}

/** Nombre à la française : virgule décimale, espace fine pour les milliers. */
export function nombre(v: number, decimales = 2): string {
  if (!Number.isFinite(v)) return "—";
  return v.toLocaleString("fr-FR", { minimumFractionDigits: 0, maximumFractionDigits: decimales });
}

export type Unite = "mm" | "mil" | "pouce";

/** Longueur (en mm) dans l'unité choisie. */
export function longueur(mm: number, unite: Unite, decimales?: number): string {
  if (unite === "mil") return `${nombre(mm / 0.0254, decimales ?? 1)} mil`;
  if (unite === "pouce") return `${nombre(mm / 25.4, decimales ?? 4)} po`;
  return `${nombre(mm, decimales ?? 3)} mm`;
}

export function pluriel(n: number, mot: string, motPluriel?: string): string {
  return `${nombre(n, 0)} ${n > 1 ? (motPluriel ?? `${mot}s`) : mot}`;
}

export function taille(octets: number): string {
  if (octets < 1024) return `${octets} o`;
  if (octets < 1024 * 1024) return `${nombre(octets / 1024, 0)} Ko`;
  return `${nombre(octets / 1024 / 1024, 1)} Mo`;
}

/** Télécharge des octets sous un nom de fichier. */
export function telecharger(donnees: BlobPart, nom: string, type: string) {
  const url = URL.createObjectURL(new Blob([donnees], { type }));
  const a = el("a", { href: url, download: nom });
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}
