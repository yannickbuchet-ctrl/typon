/** Icônes (traits SVG 24 × 24, couleur du texte). */

const ICONES: Record<string, string> = {
  ouvrir: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  dossier: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><path d="M12 11v5M9.5 13.5h5"/>',
  comparer: '<rect x="3" y="4" width="7" height="16" rx="1.5"/><rect x="14" y="4" width="7" height="16" rx="1.5"/><path d="M10 9h4M10 15h4"/>',
  oeil: '<path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>',
  oeilFerme: '<path d="M3 3l18 18M10.6 5.1A9.8 9.8 0 0 1 12 5c6.4 0 10 7 10 7a17 17 0 0 1-3.2 4M6.3 6.3A17 17 0 0 0 2 12s3.6 7 10 7a9.6 9.6 0 0 0 4.3-1"/><path d="M9.9 9.9a3 3 0 0 0 4.2 4.2"/>',
  ajuster: '<path d="M4 9V5a1 1 0 0 1 1-1h4M15 4h4a1 1 0 0 1 1 1v4M20 15v4a1 1 0 0 1-1 1h-4M9 20H5a1 1 0 0 1-1-1v-4"/>',
  mesure: '<path d="M3 17 17 3l4 4L7 21z"/><path d="M7 13l2 2M10 10l2 2M13 7l2 2"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7.5v.5"/>',
  main: '<path d="M8 13V5.5a1.5 1.5 0 0 1 3 0V12M11 11.5v-7a1.5 1.5 0 0 1 3 0V12M14 11.5V6.5a1.5 1.5 0 0 1 3 0V14a7 7 0 0 1-7 7h-.5a6 6 0 0 1-4.8-2.4L3 15.5a1.6 1.6 0 0 1 2.5-2L8 15"/>',
  retourner: '<path d="M12 3v18"/><path d="M8 7 4 12l4 5V7zM16 7l4 5-4 5V7z"/>',
  soleil: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>',
  lune: '<path d="M20 14.5A8 8 0 1 1 9.5 4 6.5 6.5 0 0 0 20 14.5z"/>',
  fermer: '<path d="M6 6l12 12M18 6 6 18"/>',
  pdf: '<path d="M6 3h8l4 4v14H6z"/><path d="M14 3v4h4M9 13h6M9 17h6"/>',
  exemple: '<path d="M4 6h16v12H4z"/><circle cx="8" cy="10" r="1.5"/><path d="M8 14h3l2-3h3"/>',
  couches: '<path d="m12 3 9 5-9 5-9-5 9-5z"/><path d="m3 12 9 5 9-5M3 16l9 5 9-5"/>',
  puce: '<rect x="6" y="6" width="12" height="12" rx="2"/><path d="M9 2v4M15 2v4M9 18v4M15 18v4M2 9h4M2 15h4M18 9h4M18 15h4"/>',
  reseau: '<circle cx="5" cy="12" r="2"/><circle cx="19" cy="6" r="2"/><circle cx="19" cy="18" r="2"/><path d="M7 12h5l5-6M12 12l5 6"/>',
  liste: '<path d="M9 6h11M9 12h11M9 18h11"/><circle cx="4.5" cy="6" r="1"/><circle cx="4.5" cy="12" r="1"/><circle cx="4.5" cy="18" r="1"/>',
  percage: '<circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="3"/>',
  fichier: '<path d="M6 3h8l4 4v14H6z"/><path d="M14 3v4h4"/>',
  echange: '<path d="M7 7h12l-3-3M17 17H5l3 3"/>',
  zoom: '<circle cx="11" cy="11" r="6"/><path d="m20 20-4.5-4.5"/>',
  alerte: '<path d="M12 3 2 20h20z"/><path d="M12 9v5M12 17v.5"/>',
  telecharger: '<path d="M12 3v12M7 10l5 5 5-5M4 21h16"/>',
  menu: '<path d="M4 6h16M4 12h16M4 18h16"/>',
  gauche: '<path d="m15 6-6 6 6 6"/>',
  droite: '<path d="m9 6 6 6-6 6"/>',
  cube: '<path d="m12 3 8 4.5v9L12 21l-8-4.5v-9z"/><path d="m4 7.5 8 4.5 8-4.5M12 12v9"/>',
  carte: '<rect x="3" y="5" width="18" height="14" rx="2"/><circle cx="8" cy="10" r="1.5"/><path d="M8 14h4l3-3h3"/>',
};

export function icone(nom: string, taille = 18): string {
  return `<svg viewBox="0 0 24 24" width="${taille}" height="${taille}" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICONES[nom] ?? ""}</svg>`;
}

/** Logo de Typon : un typon stylisé (piste et pastilles) dans un carré arrondi. */
export const LOGO = `<svg viewBox="0 0 64 64" width="30" height="30" aria-hidden="true">
<defs><linearGradient id="typon-g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#ffb347"/><stop offset="1" stop-color="#e8743b"/></linearGradient></defs>
<rect x="2" y="2" width="60" height="60" rx="14" fill="#10161f"/>
<rect x="2" y="2" width="60" height="60" rx="14" fill="none" stroke="url(#typon-g)" stroke-width="2.5" opacity=".55"/>
<path d="M16 46V30l10-10h14" fill="none" stroke="url(#typon-g)" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"/>
<path d="M40 20h8" fill="none" stroke="url(#typon-g)" stroke-width="5" stroke-linecap="round"/>
<circle cx="16" cy="46" r="6.5" fill="url(#typon-g)"/><circle cx="16" cy="46" r="2.4" fill="#10161f"/>
<rect x="38" y="34" width="12" height="12" rx="2.5" fill="url(#typon-g)"/>
<circle cx="48" cy="20" r="5" fill="#10161f" stroke="url(#typon-g)" stroke-width="3"/>
</svg>`;
