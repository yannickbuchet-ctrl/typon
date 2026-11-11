//! Typon — moteur WebAssembly : lecture des Gerber (RS-274X, X2, X3), perçages Excellon et XNC, fichiers de job,
//! netlists IPC-D-356 et ODB++, puis géométrie prête pour WebGL et informations des objets.
//!
//! Interface sans wasm-bindgen : la page alloue de la mémoire (`alloc`), y copie les fichiers, appelle les fonctions
//! exportées, puis lit les « sorties » (en-tête, JSON, données binaires) et les libère (`sortie_liberer`).

pub mod archive;
pub mod couches;
pub mod dfm;
pub mod fabrication;
pub mod excellon;
pub mod geom;
pub mod gerber;
pub mod ipc2581;
pub mod job;
pub mod odb;
pub mod ouvertures;
pub mod zones;

use archive::Fichier;
use couches::{Genre, Role};
use geom::{BBox, Dessin, Index, Prim};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

pub struct Couche {
    pub nom: String,
    pub fichier: String,
    pub format: &'static str,
    pub role: Role,
    pub dessin: Dessin,
    pub index: Option<Index>,
    pub meta: Value,
}

#[derive(Default)]
pub struct Lot {
    pub fichiers: Vec<Fichier>,
    pub couches: Vec<Couche>,
    pub meta: Value,
}

thread_local! {
    static LOTS: RefCell<HashMap<u32, Lot>> = RefCell::new(HashMap::new());
    static SUIVANT: RefCell<u32> = const { RefCell::new(1) };
}

/* --- Mémoire et sorties ------------------------------------------------------------------------------------------ */

#[unsafe(no_mangle)]
pub extern "C" fn alloc(n: usize) -> *mut u8 {
    let n = n.max(1);
    unsafe { std::alloc::alloc(std::alloc::Layout::from_size_align_unchecked(n, 1)) }
}

/// # Safety
/// `p` doit venir de `alloc(n)`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dealloc(p: *mut u8, n: usize) {
    unsafe { std::alloc::dealloc(p, std::alloc::Layout::from_size_align_unchecked(n.max(1), 1)) }
}

/// Sortie : [longueur totale u32][longueur du JSON u32][JSON][remplissage jusqu'à un multiple de 4][données].
fn sortie(json: &Value, donnees: &[u8]) -> *mut u8 {
    let j = serde_json::to_vec(json).unwrap_or_default();
    let debut = (8 + j.len()).div_ceil(4) * 4;
    let total = debut + donnees.len();
    let mut v = vec![0u8; total];
    v[0..4].copy_from_slice(&(total as u32).to_le_bytes());
    v[4..8].copy_from_slice(&(j.len() as u32).to_le_bytes());
    v[8..8 + j.len()].copy_from_slice(&j);
    v[debut..].copy_from_slice(donnees);
    let p = alloc(total);
    unsafe { std::ptr::copy_nonoverlapping(v.as_ptr(), p, total) };
    p
}

/// # Safety
/// `p` doit venir d'une fonction qui renvoie une sortie.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sortie_liberer(p: *mut u8) {
    if p.is_null() {
        return;
    }
    let total = unsafe { u32::from_le_bytes([*p, *p.add(1), *p.add(2), *p.add(3)]) } as usize;
    unsafe { dealloc(p, total) };
}

unsafe fn texte(p: *const u8, n: usize) -> String {
    if n == 0 {
        return String::new();
    }
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(p, n) }).into_owned()
}

/* --- Lots de fichiers -------------------------------------------------------------------------------------------- */

#[unsafe(no_mangle)]
pub extern "C" fn lot_nouveau() -> u32 {
    let id = SUIVANT.with(|s| {
        let mut s = s.borrow_mut();
        *s += 1;
        *s - 1
    });
    LOTS.with(|l| l.borrow_mut().insert(id, Lot::default()));
    id
}

/// Ajoute un fichier au lot ; ses octets (alloués par `alloc(n)`) appartiennent désormais au moteur.
///
/// # Safety
/// `nom` doit pointer sur `nom_n` octets ; `donnees` doit venir de `alloc(donnees_n)`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lot_ajouter(lot: u32, nom: *const u8, nom_n: usize, donnees: *mut u8, donnees_n: usize) {
    let nom = unsafe { texte(nom, nom_n) };
    let octets = unsafe { Vec::from_raw_parts(donnees, donnees_n, donnees_n.max(1)) };
    LOTS.with(|l| {
        if let Some(lot) = l.borrow_mut().get_mut(&lot) {
            lot.fichiers.push(Fichier { chemin: nom, octets });
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn lot_fermer(lot: u32) {
    LOTS.with(|l| l.borrow_mut().remove(&lot));
}

/// Lit tous les fichiers du lot ; renvoie la description des couches et les métadonnées (JSON).
#[unsafe(no_mangle)]
pub extern "C" fn lot_analyser(lot: u32) -> *mut u8 {
    LOTS.with(|l| {
        let mut l = l.borrow_mut();
        let Some(lot) = l.get_mut(&lot) else { return sortie(&json!({"erreur": "Lot inconnu."}), &[]) };
        let r = analyser(lot);
        sortie(&r, &[])
    })
}

/// Géométrie d'une couche pour WebGL.
#[unsafe(no_mangle)]
pub extern "C" fn couche_geometrie(lot: u32, couche: u32) -> *mut u8 {
    LOTS.with(|l| {
        let l = l.borrow();
        let Some(c) = l.get(&lot).and_then(|x| x.couches.get(couche as usize)) else { return sortie(&json!({"erreur": "Couche inconnue."}), &[]) };
        let (j, d) = geometrie(&c.dessin);
        sortie(&j, &d)
    })
}

/// Informations de l'objet sous le point (x, y en mm), à `tol` mm près.
#[unsafe(no_mangle)]
pub extern "C" fn objet_sous(lot: u32, couche: u32, x: f64, y: f64, tol: f64) -> *mut u8 {
    LOTS.with(|l| {
        let mut l = l.borrow_mut();
        let Some(c) = l.get_mut(&lot).and_then(|x| x.couches.get_mut(couche as usize)) else { return sortie(&Value::Null, &[]) };
        if c.index.is_none() {
            c.index = Some(Index::nouveau(&c.dessin));
        }
        let objet = c.index.as_ref().unwrap().objet_sous(&c.dessin, geom::pt(x, y), tol);
        match objet.and_then(|o| c.dessin.info_objet(o).map(|i| (o, i))) {
            Some((o, info)) => {
                // Contour de l'objet, pour le surligner : boîte englobante de ses éléments.
                let mut b = BBox::vide();
                for e in c.dessin.elements.iter().filter(|e| e.objet == o) {
                    b.union(&e.prim.boite_englobante());
                }
                sortie(&json!({"objet": o, "info": info, "boite": b.tableau()}), &[])
            }
            None => sortie(&Value::Null, &[]),
        }
    })
}

/// Contour de la carte pour la vue 3D : anneaux fermés (le plus grand est le bord, les autres des découpes) et
/// triangulation de la face (x, y en f32). Sans contour fermé : le rectangle englobant les couches de cuivre.
#[unsafe(no_mangle)]
pub extern "C" fn modele_3d(lot: u32) -> *mut u8 {
    LOTS.with(|l| {
        let l = l.borrow();
        let Some(lot) = l.get(&lot) else { return sortie(&json!({"erreur": "Lot inconnu."}), &[]) };
        let mut anneaux: Vec<Vec<geom::Pt>> = Vec::new();
        for c in lot.couches.iter().filter(|c| c.role.genre == Genre::Contour) {
            anneaux.extend(geom::anneaux_contour(&c.dessin));
            if !anneaux.is_empty() {
                break;
            }
        }
        let mut source = "contour";
        if anneaux.is_empty() {
            let mut b = BBox::vide();
            for c in lot.couches.iter().filter(|c| c.role.genre == Genre::Cuivre) {
                b.union(&c.dessin.boite());
            }
            if b.est_vide() {
                for c in &lot.couches {
                    b.union(&c.dessin.boite());
                }
            }
            b.gonfle(1.0);
            anneaux.push(vec![geom::pt(b.x0, b.y0), geom::pt(b.x1, b.y0), geom::pt(b.x1, b.y1), geom::pt(b.x0, b.y1)]);
            source = "rectangle";
        }
        // Le bord : l'anneau de plus grande aire ; les découpes : les anneaux à l'intérieur du bord.
        anneaux.sort_by(|a, b| geom::aire(b).abs().total_cmp(&geom::aire(a).abs()));
        let bord = anneaux[0].clone();
        let decoupes: Vec<Vec<geom::Pt>> = anneaux[1..].iter().filter(|a| geom::point_dans_anneau(a[0], &bord)).cloned().collect();
        let mut tous = vec![bord];
        tous.extend(decoupes);
        let mut tris: Vec<f32> = Vec::new();
        geom::trianguler(&tous, &mut tris);
        let donnees: Vec<u8> = tris.iter().flat_map(|v| v.to_le_bytes()).collect();
        let anneaux_json: Vec<Value> = tous.iter().map(|a| json!(a.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>())).collect();
        sortie(&json!({"anneaux": anneaux_json, "source": source}), &donnees)
    })
}

/// Le net `nom` porte-t-il cet objet ?
fn objet_du_net(d: &Dessin, objet: u32, nom: &str) -> bool {
    d.info_objet(objet).map(|i| i.iter().any(|(k, v)| k == "Net" && v == nom)).unwrap_or(false)
}

/// Géométrie (pour WebGL) des seuls éléments du net `nom` sur une couche : pour le surligner.
///
/// # Safety
/// `nom` doit pointer sur `nom_n` octets UTF-8 alloués par `alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn net_geometrie(lot: u32, couche: u32, nom: *const u8, nom_n: usize) -> *mut u8 {
    let nom = unsafe { texte(nom, nom_n) };
    LOTS.with(|l| {
        let l = l.borrow();
        let Some(c) = l.get(&lot).and_then(|x| x.couches.get(couche as usize)) else { return sortie(&json!({"erreur": "Couche inconnue."}), &[]) };
        let mut d = Dessin::default();
        let o = d.nouvel_objet(Vec::new());
        for e in c.dessin.elements.iter().filter(|e| e.sombre && objet_du_net(&c.dessin, e.objet, &nom)) {
            d.ajoute(e.prim.clone(), true, o);
        }
        let (j, octets) = geometrie(&d);
        sortie(&j, &octets)
    })
}

/// Un net : longueur de ses pistes (axe des segments) par couche de cuivre, nombre de pastilles et de trous.
///
/// # Safety
/// `nom` doit pointer sur `nom_n` octets UTF-8 alloués par `alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn net_info(lot: u32, nom: *const u8, nom_n: usize) -> *mut u8 {
    let nom = unsafe { texte(nom, nom_n) };
    LOTS.with(|l| {
        let l = l.borrow();
        let Some(lot) = l.get(&lot) else { return sortie(&json!({"erreur": "Lot inconnu."}), &[]) };
        let mut couches = Vec::new();
        let (mut total, mut trous) = (0.0, 0usize);
        let mut boite = BBox::vide();
        for (id, c) in lot.couches.iter().enumerate() {
            let (mut longueur, mut traits, mut pastilles, mut surfaces) = (0.0, 0usize, 0usize, 0usize);
            for e in c.dessin.elements.iter().filter(|e| e.sombre && objet_du_net(&c.dessin, e.objet, &nom)) {
                boite.union(&e.prim.boite_englobante());
                match &e.prim {
                    Prim::Cap { a, b, .. } if a.dist(*b) > 1e-9 => {
                        longueur += a.dist(*b);
                        traits += 1;
                    }
                    Prim::Poly { .. } => surfaces += 1,
                    _ => pastilles += 1,
                }
            }
            if traits + pastilles + surfaces == 0 {
                continue;
            }
            if c.role.genre == Genre::Percage || c.role.genre == Genre::Fraisage {
                trous += pastilles + traits;
            } else if c.role.genre == Genre::Cuivre {
                total += longueur;
            }
            couches.push(json!({"id": id, "nom": c.nom, "genre": c.role.genre, "longueur": longueur, "traits": traits, "pastilles": pastilles, "surfaces": surfaces}));
        }
        sortie(&json!({"nom": nom, "couches": couches, "longueur": total, "trous": trous, "boite": if boite.est_vide() { Value::Null } else { json!(boite.tableau()) }}), &[])
    })
}

/// Vérifications de fabricabilité (voir dfm.rs) ; `regles` : seuils en JSON.
///
/// # Safety
/// `regles` doit pointer sur `regles_n` octets UTF-8 alloués par `alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dfm_verifier(lot: u32, regles: *const u8, regles_n: usize) -> *mut u8 {
    let regles: Value = serde_json::from_str(&unsafe { texte(regles, regles_n) }).unwrap_or(Value::Null);
    LOTS.with(|l| {
        let l = l.borrow();
        let Some(lot) = l.get(&lot) else { return sortie(&json!({"erreur": "Lot inconnu."}), &[]) };
        sortie(&dfm::verifier(&lot.couches, &dfm::Regles::depuis(&regles)), &[])
    })
}

/// Panneau (voir fabrication.rs) : fichiers Gerber X2 et Excellon du panneau, dans une archive ZIP (données).
///
/// # Safety
/// `params` doit pointer sur `params_n` octets UTF-8 alloués par `alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn panneau(lot: u32, params: *const u8, params_n: usize) -> *mut u8 {
    let params: Value = serde_json::from_str(&unsafe { texte(params, params_n) }).unwrap_or(Value::Null);
    LOTS.with(|l| {
        let l = l.borrow();
        let Some(lot) = l.get(&lot) else { return sortie(&json!({"erreur": "Lot inconnu."}), &[]) };
        match fabrication::panneau(&lot.couches, &params) {
            Ok((j, zip)) => sortie(&j, &zip),
            Err(e) => sortie(&json!({"erreur": e}), &[]),
        }
    })
}

/// Zones de différence d'un masque de comparaison (voir zones.rs).
///
/// # Safety
/// `masque` doit pointer sur `largeur × hauteur` octets.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn zones_difference(masque: *const u8, largeur: usize, hauteur: usize, fusion: usize, minimum: u32) -> *mut u8 {
    let m = unsafe { std::slice::from_raw_parts(masque, largeur * hauteur) };
    let z = zones::zones(m, largeur, hauteur, fusion, minimum);
    sortie(&json!(z), &[])
}

/* --- Analyse ----------------------------------------------------------------------------------------------------- */

pub fn geometrie(dessin: &Dessin) -> (Value, Vec<u8>) {
    let lots = geom::lots(dessin);
    let mut donnees: Vec<u8> = Vec::new();
    let mut desc = Vec::new();
    let mut ajoute = |v: &[f32], donnees: &mut Vec<u8>| -> [usize; 2] {
        let debut = donnees.len() / 4;
        for x in v {
            donnees.extend_from_slice(&x.to_le_bytes());
        }
        [debut, v.len()]
    };
    for lot in &lots {
        let caps = ajoute(&lot.caps, &mut donnees);
        let boites = ajoute(&lot.boites, &mut donnees);
        let tris = ajoute(&lot.tris, &mut donnees);
        desc.push(json!({"sombre": lot.sombre, "caps": caps, "boites": boites, "tris": tris}));
    }
    (json!({"lots": desc, "boite": dessin.boite().tableau()}), donnees)
}

pub fn analyser(lot: &mut Lot) -> Value {
    let mut alertes: Vec<String> = Vec::new();
    let mut fichiers: Vec<Fichier> = Vec::new();
    for f in std::mem::take(&mut lot.fichiers) {
        match archive::developper(&f.chemin, f.octets) {
            Ok(v) => fichiers.extend(v),
            Err(e) => alertes.push(format!("{} : {e}", f.chemin)),
        }
    }
    let mut meta = serde_json::Map::new();
    let mut couches: Vec<Couche> = Vec::new();
    let mut ignores: Vec<String> = Vec::new();

    // ODB++
    if odb::reconnaitre(&fichiers) {
        match odb::lire(&fichiers) {
            Ok(o) => ajouter_odb(o, &mut couches, &mut meta),
            Err(e) => alertes.push(e),
        }
        fichiers.retain(|f| {
            let c = f.chemin.to_ascii_lowercase();
            !(c.contains("/steps/") || c.contains("/matrix/") || c.contains("/misc/") || c.contains("/symbols/") || c.contains("/fonts/") || c.starts_with("steps/") || c.starts_with("matrix/") || c.starts_with("misc/") || c.contains("/input/") || c.contains("/user/") || c.contains("/ext/") || c.contains("/wheels/") || c.contains("/output/"))
        });
    }

    // Fichier de job d'abord : il donne le rôle des fichiers Gerber sans attributs X2.
    let mut roles_job: HashMap<String, Vec<String>> = HashMap::new();
    for f in &fichiers {
        if f.chemin.to_ascii_lowercase().ends_with(".gbrjob") {
            match job::lire(&String::from_utf8_lossy(&f.octets)) {
                Ok(j) => {
                    for (chemin, fonction) in &j.fichiers {
                        roles_job.insert(nom_court(chemin).to_ascii_lowercase(), fonction.clone());
                    }
                    if let Some(e) = j.epaisseur {
                        meta.insert("epaisseur".into(), json!({"valeur": e, "source": "fichier de job"}));
                    }
                    if let Some(f) = &j.finition {
                        meta.insert("finition".into(), json!(f));
                    }
                    meta.insert("job".into(), json!({
                        "fichier": nom_court(&f.chemin),
                        "rubriques": j.rubriques.iter().map(|(t, l)| json!({"titre": t, "lignes": l})).collect::<Vec<_>>(),
                        "empilage": j.empilage.iter().map(|(n, t, e, m)| json!({"nom": n, "type": t, "epaisseur": e, "materiau": m})).collect::<Vec<_>>(),
                    }));
                }
                Err(e) => alertes.push(e),
            }
        }
    }

    // Listes d'ouvertures des Gerber RS-274D (roues Eagle, listes Altium/Protel, tableaux de D-codes…).
    let mut listes: Vec<(String, ouvertures::Liste)> = Vec::new();
    for f in &fichiers {
        let nom = nom_court(&f.chemin).to_string();
        let texte = String::from_utf8_lossy(&f.octets);
        if !est_ignorable(&nom.to_ascii_lowercase()) && !excellon::reconnaitre(&texte) && ouvertures::reconnaitre(&nom, &texte) {
            if let Some(l) = ouvertures::lire(&texte) {
                listes.push((nom, l));
            }
        }
    }
    if !listes.is_empty() {
        meta.insert("listesOuvertures".into(), json!(listes.iter().map(|(n, l)| json!({"fichier": n, "ouvertures": l.ouvertures.len(), "unite": l.unite})).collect::<Vec<_>>()));
    }
    let mut composants: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut nets_gerber: Vec<String> = Vec::new();
    for f in &fichiers {
        let nom = nom_court(&f.chemin).to_string();
        let bas = nom.to_ascii_lowercase();
        if bas.ends_with(".gbrjob") || listes.iter().any(|(n, _)| *n == nom) {
            continue;
        }
        if est_ignorable(&bas) || f.octets.is_empty() {
            ignores.push(nom);
            continue;
        }
        let texte = String::from_utf8_lossy(&f.octets);
        if ipc2581::reconnaitre(&texte) {
            match ipc2581::lire(&texte) {
                Ok(o) => ajouter_odb(o, &mut couches, &mut meta),
                Err(e) => alertes.push(format!("{nom} : {e}")),
            }
            continue;
        }
        if est_ipc356(&bas, &texte) {
            let nets = ipc356_nets(&texte);
            meta.insert("netlist".into(), json!({"fichier": nom, "nets": nets.len()}));
            for n in nets {
                if !nets_gerber.contains(&n) {
                    nets_gerber.push(n);
                }
            }
            continue;
        }
        if excellon::reconnaitre(&texte) {
            let e = excellon::lire(&texte);
            let role = e.attributs.iter().find(|(n, _)| n == "FileFunction").and_then(|(_, v)| couches::depuis_x2(v))
                .or_else(|| roles_job.get(&bas).and_then(|v| couches::depuis_x2(v)))
                .unwrap_or_else(|| {
                    let mut r = couches::depuis_nom(&nom);
                    if r.genre != Genre::Percage && r.genre != Genre::Fraisage {
                        r = couches::depuis_x2(&["Plated".into()]).unwrap();
                    }
                    r.libelle = if bas.contains("npth") || bas.contains("non") { "Perçage non métallisé".into() } else if bas.contains("pth") || bas.contains("plated") { "Perçage métallisé".into() } else { "Perçage".into() };
                    r.source = "nom de fichier";
                    r
                });
            let trous: usize = e.outils.iter().map(|o| o.trous).sum();
            let meta_c = json!({
                "format": "Excellon",
                "unite": e.unite,
                "notation": e.format,
                "attributs": e.attributs.iter().map(|(n, v)| json!([libelle_tf(n), traduire_tf(n, v)])).collect::<Vec<_>>(),
                "commentaires": e.commentaires,
                "outils": e.outils.iter().map(|o| json!({"numero": o.numero, "diametre": o.diametre, "trous": o.trous, "fraisages": o.fraisages})).collect::<Vec<_>>(),
                "trous": trous,
                "alertes": e.alertes,
            });
            couches.push(Couche { nom: role.libelle.clone(), fichier: nom, format: "Excellon", role, dessin: e.dessin, index: None, meta: meta_c });
            continue;
        }
        if est_gerber(&texte) {
            // RS-274D (ni format ni ouvertures dans le fichier) : on lui associe la liste d'ouvertures au nom le plus
            // proche.
            let rs274d = !texte.contains("%FS") && !texte.contains("%AD");
            let liste = if rs274d { listes.iter().max_by_key(|(n, _)| prefixe_commun(n, &nom)) } else { None };
            let g = match liste {
                Some((n, l)) => gerber::lire_rs274d(&texte, l, n),
                None => gerber::lire(&texte),
            };
            let fonction = g.attributs.iter().find(|(n, _)| n == "FileFunction").map(|(_, v)| v.clone());
            let mut role = fonction.as_ref().and_then(|v| couches::depuis_x2(v))
                .or_else(|| roles_job.get(&bas).and_then(|v| couches::depuis_x2(v)))
                .unwrap_or_else(|| couches::depuis_nom(&nom));
            if role.genre == Genre::Autre && role.libelle == "Autre" {
                role.libelle = nom.clone();
            }
            // Gerber X3 : la position du composant est celle de son flash « ComponentMain ».
            // Encombrement : axes du contour « ComponentOutline » (le corps « Body » de préférence).
            let mut positions: HashMap<String, (f64, f64)> = HashMap::new();
            let mut contours: HashMap<String, (u8, Vec<geom::Pt>)> = HashMap::new();
            for e in &g.dessin.elements {
                if let Some(info) = g.dessin.info_objet(e.objet) {
                    let Some((_, r)) = info.iter().find(|(k, _)| k == "Composant") else { continue };
                    let Some((_, f)) = info.iter().find(|(k, _)| k == "Fonction") else { continue };
                    if f.starts_with("ComponentMain") {
                        let b = e.prim.boite_englobante();
                        positions.insert(r.clone(), ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0));
                    } else if f.starts_with("ComponentOutline") {
                        let rang = if f.contains("Body") { 0 } else if f.contains("Lead2Lead") { 1 } else if f.contains("Footprint") { 2 } else { 3 };
                        let points: Vec<geom::Pt> = match &e.prim {
                            Prim::Cap { a, b, .. } => vec![*a, *b],
                            autre => {
                                let b = autre.boite_englobante();
                                vec![geom::pt(b.x0, b.y0), geom::pt(b.x1, b.y1)]
                            }
                        };
                        let c = contours.entry(r.clone()).or_insert((rang, Vec::new()));
                        if rang < c.0 {
                            *c = (rang, Vec::new());
                        }
                        if rang == c.0 {
                            c.1.extend(points);
                        }
                    }
                }
            }
            for (r, a) in &g.composants {
                let e = composants.entry(r.clone()).or_default();
                if let Some((x, y)) = positions.get(r) {
                    e.insert("__x".into(), x.to_string());
                    e.insert("__y".into(), y.to_string());
                }
                if let Some((_, pts)) = contours.get(r) {
                    e.insert("__contour".into(), pts.iter().map(|p| format!("{} {}", p.x, p.y)).collect::<Vec<_>>().join(";"));
                }
                for (k, v) in a {
                    e.insert(k.clone(), v.clone());
                }
                if let Some(c) = fonction.as_ref().and_then(|f| f.get(2)) {
                    e.insert("Côté".into(), if c == "Bot" { "dessous".into() } else { "dessus".into() });
                }
            }
            for n in &g.nets {
                if !nets_gerber.contains(n) {
                    nets_gerber.push(n.clone());
                }
            }
            let nb = g.dessin.objets.len();
            let meta_c = json!({
                "format": if rs274d { "Gerber RS-274D" } else if g.attributs.is_empty() { "Gerber RS-274X" } else if !g.composants.is_empty() { "Gerber X3" } else { "Gerber X2" },
                "unite": g.unite,
                "notation": g.format,
                "attributs": g.attributs.iter().map(|(n, v)| json!([libelle_tf(n), traduire_tf(n, v)])).collect::<Vec<_>>(),
                "commentaires": g.commentaires,
                "ouvertures": g.ouvertures.iter().map(|(n, d, f)| json!({"code": format!("D{n}"), "forme": d, "fonction": f})).collect::<Vec<_>>(),
                "objets": nb,
                "nets": g.nets.len(),
                "composants": g.composants.len(),
                "negatif": g.negatif,
                "alertes": g.alertes,
            });
            couches.push(Couche { nom: role.libelle.clone(), fichier: nom, format: "Gerber", role, dessin: g.dessin, index: None, meta: meta_c });
            continue;
        }
        ignores.push(nom);
    }

    // Couches cuivre sans numéro : numérotées d'après leur côté.
    couches.sort_by(|a, b| b.role.rang.cmp(&a.role.rang).then(a.nom.cmp(&b.nom)));
    // Couches tronquées au plafond d'éléments (fichier démesuré ou piégé).
    for c in couches.iter().filter(|c| c.dessin.tronque) {
        alertes.push(format!("« {} » : plus de {} éléments, la suite du fichier est ignorée.", c.fichier, geom::LIMITE_ELEMENTS));
    }
    // Noms en double (deux fichiers « Cuivre dessus ») : on ajoute le nom du fichier.
    let mut vus: HashMap<String, usize> = HashMap::new();
    for c in &couches {
        *vus.entry(c.nom.clone()).or_default() += 1;
    }
    for c in couches.iter_mut() {
        if vus[&c.nom] > 1 {
            c.nom = format!("{} ({})", c.nom, c.fichier);
        }
    }

    if !composants.is_empty() && !meta.contains_key("composants") {
        let liste: Vec<Value> = composants.iter().map(|(r, a)| {
            // Encombrement dans le repère du composant : contour ramené à sa position, rotation annulée, miroir au
            // dessous (comme une empreinte ODB++).
            let x = a.get("__x").and_then(|v| v.parse::<f64>().ok());
            let y = a.get("__y").and_then(|v| v.parse::<f64>().ok());
            let rot = a.get("rotation").and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0).to_radians();
            let dessous = a.get("Côté").map(|c| c == "dessous").unwrap_or(false);
            let corps = match (x, y, a.get("__contour")) {
                (Some(x), Some(y), Some(c)) => {
                    let mut b = BBox::vide();
                    for p in c.split(';') {
                        let mut v = p.split(' ').filter_map(|n| n.parse::<f64>().ok());
                        if let (Some(px), Some(py)) = (v.next(), v.next()) {
                            let (dx, dy) = (px - x, py - y);
                            let lx = dx * rot.cos() + dy * rot.sin();
                            let ly = -dx * rot.sin() + dy * rot.cos();
                            b.ajoute(geom::pt(if dessous { -lx } else { lx }, ly));
                        }
                    }
                    (!b.est_vide() && b.x1 > b.x0 && b.y1 > b.y0).then(|| b.tableau())
                }
                _ => None,
            };
            json!({
                "corps": corps,
                "reference": r,
                "valeur": a.get("valeur").cloned().unwrap_or_default(),
                "boitier": a.get("empreinte").or(a.get("boîtier")).cloned().unwrap_or_default(),
                "piece": a.get("référence fabricant").cloned().unwrap_or_default(),
                "fabricant": a.get("fabricant").cloned().unwrap_or_default(),
                "rotation": a.get("rotation").cloned().unwrap_or_default(),
                "cote": a.get("Côté").cloned().unwrap_or_default(),
                "x": a.get("__x").and_then(|v| v.parse::<f64>().ok()),
                "y": a.get("__y").and_then(|v| v.parse::<f64>().ok()),
                "hauteur": a.get("hauteur").and_then(|v| v.split(|c: char| !(c.is_ascii_digit() || c == '.')).next()).and_then(|v| v.parse::<f64>().ok()),
                "proprietes": a.iter().filter(|(k, _)| !k.starts_with("__")).map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
            })
        }).collect();
        meta.insert("composants".into(), json!(liste));
    }
    if !nets_gerber.is_empty() && !meta.contains_key("nets") {
        nets_gerber.sort();
        meta.insert("nets".into(), json!(nets_gerber.iter().map(|n| json!({"nom": n, "elements": Value::Null})).collect::<Vec<_>>()));
    }

    // Dimensions : contour s'il existe, sinon toutes les couches.
    let mut boite = BBox::vide();
    for c in couches.iter().filter(|c| c.role.genre == Genre::Contour) {
        boite.union(&c.dessin.boite_axes());
    }
    if boite.est_vide() {
        for c in &couches {
            boite.union(&c.dessin.boite());
        }
    }
    meta.insert("boite".into(), json!(boite.tableau()));
    meta.insert("couchesCuivre".into(), json!(couches.iter().filter(|c| c.role.genre == Genre::Cuivre).count()));

    let description: Vec<Value> = couches.iter().enumerate().map(|(i, c)| {
        json!({
            "id": i,
            "nom": c.nom,
            "fichier": c.fichier,
            "format": c.format,
            "role": c.role,
            "boite": c.dessin.boite().tableau(),
            "objets": c.dessin.objets.len(),
            "meta": c.meta,
        })
    }).collect();
    lot.couches = couches;
    lot.meta = Value::Object(meta.clone());
    json!({"couches": description, "meta": meta, "alertes": alertes, "ignores": ignores})
}

fn ajouter_odb(o: odb::Odb, couches: &mut Vec<Couche>, meta: &mut serde_json::Map<String, Value>) {
    let mut bom: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for c in &o.composants {
        bom.entry((c.piece.clone(), c.valeur.clone())).or_default().push(c.reference.clone());
    }
    meta.insert("odb".into(), json!({
        "source": o.format,
        "info": o.info,
        "etape": o.etape,
        "etapes": o.etapes,
        "empilage": o.couches.iter().map(|c| json!({"nom": c.nom, "type": c.type_odb, "contexte": c.contexte, "role": c.role.libelle, "debut": c.debut, "fin": c.fin, "negative": c.polarite_negative, "elements": c.elements})).collect::<Vec<_>>(),
        "outils": o.couches.iter().filter(|c| !c.outils.is_empty()).map(|c| json!({"couche": c.nom, "debut": c.debut, "fin": c.fin, "outils": c.outils.iter().map(|t| json!({"numero": t.numero, "genre": t.genre, "fini": t.fini, "percage": t.percage, "trous": t.trous})).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "alertes": o.alertes,
    }));
    meta.insert("composants".into(), json!(o.composants.iter().map(|c| json!({
        "reference": c.reference,
        "valeur": c.valeur,
        "boitier": c.boitier,
        "piece": c.piece,
        "x": c.x,
        "y": c.y,
        // ODB++ compte les rotations dans le sens horaire ; Typon les donne dans le sens trigonométrique, comme X3.
        "rotation": (360.0 - c.rotation).rem_euclid(360.0),
        "cote": if c.dessous { "dessous" } else { "dessus" },
        "broches": c.broches,
        "proprietes": c.proprietes,
        "corps": c.corps,
        "hauteur": c.hauteur,
    })).collect::<Vec<_>>()));
    if let Some(e) = o.epaisseur {
        meta.insert("epaisseur".into(), json!({"valeur": e, "source": o.format}));
    }
    meta.insert("nomenclature".into(), json!(bom.iter().map(|((p, v), refs)| json!({"piece": p, "valeur": v, "quantite": refs.len(), "references": refs})).collect::<Vec<_>>()));
    meta.insert("nets".into(), json!(o.nets.iter().map(|(n, k)| json!({"nom": n, "elements": k})).collect::<Vec<_>>()));
    for mut c in o.couches {
        // Perçage : métallisé ou non d'après les outils (PLATED, VIA ou NON_PLATED), et couches reliées.
        if (c.role.genre == Genre::Percage || c.role.genre == Genre::Fraisage) && !c.outils.is_empty() {
            let non = c.outils.iter().all(|t| t.genre == "non métallisé");
            let base = if c.role.genre == Genre::Fraisage { "Fraisage" } else { "Perçage" };
            let etendue = if !c.debut.is_empty() { format!(" {}–{}", c.debut, c.fin) } else { String::new() };
            c.role.libelle = format!("{base} {}{etendue}", if non { "non métallisé" } else { "métallisé" });
        }
        let meta_c = json!({
            "format": o.format,
            "typeOdb": c.type_odb,
            "contexte": c.contexte,
            "nomOdb": c.nom,
            "debut": c.debut,
            "fin": c.fin,
            "negative": c.polarite_negative,
            "elements": c.elements,
            // Métallisation par outil (diamètre de perçage en mm) : un fichier peut mêler les deux.
            "outils": c.outils.iter().map(|t| json!([t.percage, t.genre != "non métallisé"])).collect::<Vec<_>>(),
        });
        let nom = c.role.libelle.clone();
        let fichier = if o.format == "ODB++" { format!("steps/{}/layers/{}", o.etape, c.nom) } else { format!("{} · {}", o.format, c.nom) };
        couches.push(Couche { nom, fichier, format: o.format, role: c.role, dessin: c.dessin, index: None, meta: meta_c });
    }
}

fn prefixe_commun(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x.eq_ignore_ascii_case(y)).count()
}

fn nom_court(chemin: &str) -> &str {
    chemin.rsplit(['/', '\\']).next().unwrap_or(chemin)
}

fn est_ignorable(bas: &str) -> bool {
    let ext = bas.rsplit('.').next().unwrap_or("");
    matches!(ext, "png" | "jpg" | "jpeg" | "gif" | "bmp" | "pdf" | "doc" | "docx" | "xls" | "xlsx" | "html" | "htm" | "csv" | "md" | "json" | "zip" | "rar" | "7z" | "exe" | "dll" | "step" | "stp" | "wrl" | "kicad_pcb" | "kicad_sch" | "kicad_pro" | "sch" | "brd" | "pcbdoc" | "schdoc" | "sum" | "lck")
        || bas.starts_with('.')
        || bas == "readme.txt"
        || bas == "readme"
}

fn est_gerber(t: &str) -> bool {
    let debut: String = t.chars().take(20_000).collect();
    debut.contains("%FS") || debut.contains("%MO") || debut.contains("%AD") || debut.contains("%TF") || (debut.contains("D01*") || debut.contains("D03*") || debut.contains("D02*")) && debut.contains('*')
}

fn est_ipc356(bas: &str, t: &str) -> bool {
    (bas.ends_with(".ipc") || bas.ends_with(".356") || bas.ends_with(".d356") || bas.contains("ipc-d-356") || bas.contains("ipc356"))
        && t.lines().take(50).any(|l| l.starts_with("P  ") || l.starts_with("C  ") || l.starts_with("317") || l.starts_with("327"))
}

fn ipc356_nets(t: &str) -> Vec<String> {
    let mut nets: Vec<String> = t
        .lines()
        .filter(|l| l.starts_with("317") || l.starts_with("327") || l.starts_with("017") || l.starts_with("027"))
        .filter_map(|l| l.get(3..17).map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty() && s != "N/C")
        .collect();
    nets.sort();
    nets.dedup();
    nets
}

fn libelle_tf(n: &str) -> String {
    match n {
        "FileFunction" => "Fonction du fichier",
        "FilePolarity" => "Polarité",
        "GenerationSoftware" => "Logiciel",
        "CreationDate" => "Date de création",
        "ProjectId" => "Projet",
        "Part" => "Partie",
        "SameCoordinates" => "Mêmes coordonnées que",
        "MD5" => "Empreinte MD5",
        "AperFunction" => "Fonction de l'ouverture",
        _ => n,
    }
    .to_string()
}

fn traduire_tf(n: &str, v: &[String]) -> String {
    match n {
        "FilePolarity" => match v.first().map(|s| s.as_str()) {
            Some("Positive") => "positive".into(),
            Some("Negative") => "négative".into(),
            _ => v.join(", "),
        },
        "Part" => match v.first().map(|s| s.as_str()) {
            Some("Single") => "carte seule".into(),
            Some("Array") => "panneau".into(),
            Some("FabricationPanel") => "panneau de fabrication".into(),
            Some("Coupon") => "coupon de test".into(),
            _ => v.join(", "),
        },
        "CreationDate" => {
            // 2019-06-03T14:22:52+02:00 -> 03/06/2019 14:22
            let s = v.join(",");
            crate::job::date_lisible(&s).unwrap_or(s)
        }
        "GenerationSoftware" => v.iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" "),
        _ => v.join(", "),
    }
}
