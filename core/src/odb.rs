//! Lecture des données ODB++ (version 7 et 8) : matrice des couches, étapes, éléments graphiques (pastilles, traits,
//! arcs, surfaces, textes), symboles standard et définis par l'utilisateur, composants et boîtiers, nets, outils de
//! perçage, informations du fichier.
//!
//! Référence : ODB++ Format Specification v8.1 update 4 (Siemens).

use crate::archive::Fichier;
use crate::couches::{self, Cote, Genre, Role};
use crate::geom::*;
use crate::gerber::nombre;
use std::collections::{BTreeMap, HashMap};
use std::f64::consts::PI;

pub struct CoucheOdb {
    pub nom: String,
    pub type_odb: String,
    pub contexte: String,
    pub polarite_negative: bool,
    pub debut: String,
    pub fin: String,
    pub role: Role,
    pub dessin: Dessin,
    pub elements: usize,
    pub outils: Vec<OutilOdb>,
}

pub struct OutilOdb {
    pub numero: String,
    pub genre: String,
    pub fini: f64,
    pub percage: f64,
    pub trous: usize,
}

pub struct Composant {
    pub reference: String,
    pub boitier: String,
    pub piece: String,
    pub valeur: String,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub dessous: bool,
    pub broches: usize,
    pub proprietes: Vec<(String, String)>,
}

pub struct Odb {
    pub info: Vec<(String, String)>,
    pub etape: String,
    pub etapes: Vec<String>,
    pub couches: Vec<CoucheOdb>,
    pub composants: Vec<Composant>,
    /// Nets : (nom, nombre d'éléments reliés)
    pub nets: Vec<(String, usize)>,
    pub alertes: Vec<String>,
    pub unite: String,
}

/// Accès aux fichiers de l'archive, par chemin relatif à la racine du produit ODB++, sans tenir compte de la casse.
struct Arbre<'a> {
    fichiers: HashMap<String, &'a [u8]>,
}

impl<'a> Arbre<'a> {
    fn lire(&self, chemin: &str) -> Option<&'a [u8]> {
        let c = chemin.to_ascii_lowercase();
        self.fichiers.get(&c).copied().or_else(|| self.fichiers.get(&format!("{c}.z")).copied())
    }
    fn texte(&self, chemin: &str) -> Option<String> {
        self.lire(chemin).map(|o| String::from_utf8_lossy(o).into_owned())
    }
    fn existe(&self, chemin: &str) -> bool {
        self.lire(chemin).is_some()
    }
    fn sous_dossiers(&self, dossier: &str) -> Vec<String> {
        let d = format!("{}/", dossier.to_ascii_lowercase());
        let mut r: Vec<String> = self.fichiers.keys().filter_map(|k| k.strip_prefix(&d)).filter_map(|s| s.split('/').next()).map(|s| s.to_string()).collect();
        r.sort();
        r.dedup();
        r
    }
}

/// Les fichiers forment-ils un produit ODB++ ?
pub fn reconnaitre(fichiers: &[Fichier]) -> bool {
    fichiers.iter().any(|f| f.chemin.to_ascii_lowercase().replace('\\', "/").ends_with("matrix/matrix"))
}

pub fn lire(fichiers: &[Fichier]) -> Result<Odb, String> {
    let racine = fichiers
        .iter()
        .map(|f| f.chemin.replace('\\', "/"))
        .find(|c| c.to_ascii_lowercase().ends_with("matrix/matrix"))
        .map(|c| c[..c.len() - "matrix/matrix".len()].to_ascii_lowercase())
        .ok_or("Pas de fichier matrix/matrix : ce n'est pas un produit ODB++.")?;
    let mut map = HashMap::new();
    let mut decompresses: Vec<(String, Vec<u8>)> = Vec::new();
    for f in fichiers {
        let c = f.chemin.replace('\\', "/").to_ascii_lowercase();
        if let Some(rel) = c.strip_prefix(&racine) {
            if crate::archive::est_lzw(&f.octets) {
                if let Ok(d) = crate::archive::lzw(&f.octets) {
                    decompresses.push((rel.trim_end_matches(".z").to_string(), d));
                    continue;
                }
            }
            map.insert(rel.to_string(), f.octets.as_slice());
        }
    }
    for (c, d) in &decompresses {
        map.insert(c.clone(), d.as_slice());
    }
    let arbre = Arbre { fichiers: map };
    let mut alertes = Vec::new();

    // Informations du fichier
    let mut info = Vec::new();
    let mut unite_defaut = "INCH".to_string();
    if let Some(t) = arbre.texte("misc/info") {
        for l in t.lines() {
            if let Some((k, v)) = l.split_once('=') {
                let (k, v) = (k.trim(), v.trim());
                if k == "UNITS" {
                    unite_defaut = v.to_ascii_uppercase();
                }
                info.push((libelle_info(k), formater_info(k, v)));
            }
        }
    }

    // Matrice
    let matrice = arbre.texte("matrix/matrix").ok_or("Fichier matrix/matrix illisible.")?;
    let blocs = blocs(&matrice);
    let mut etapes: Vec<String> = blocs.iter().filter(|(n, _)| n == "STEP").filter_map(|(_, c)| c.get("NAME").cloned()).map(|s| s.to_ascii_lowercase()).collect();
    if etapes.is_empty() {
        etapes = arbre.sous_dossiers("steps");
    }
    let mut lignes: Vec<&BTreeMap<String, String>> = blocs.iter().filter(|(n, _)| n == "LAYER").map(|(_, c)| c).collect();
    lignes.sort_by_key(|c| c.get("ROW").and_then(|r| r.parse::<i32>().ok()).unwrap_or(0));

    // Étape affichée : celle qui en répète d'autres (panneau) n'est choisie que s'il n'y a qu'elle.
    let etape = choisir_etape(&arbre, &etapes);

    // Polices et symboles utilisateur
    let police = arbre.texte("fonts/standard").map(|t| Police::lire(&t));
    let mut symboles = Symboles { arbre: &arbre, cache: HashMap::new(), alertes: Vec::new() };

    // Nets (eda/data) : nom -> éléments ; et correspondance couche/élément -> net.
    let (nets, nets_par_element) = lire_nets(&arbre, &etape);

    // Rôles : position des couches cuivre dans l'empilage.
    let cuivres: Vec<usize> = lignes.iter().enumerate().filter(|(_, c)| est_cuivre(c)).map(|(i, _)| i).collect();
    let premier = cuivres.first().copied().unwrap_or(0);
    let dernier = cuivres.last().copied().unwrap_or(usize::MAX);

    let mut couches_odb = Vec::new();
    for (i, c) in lignes.iter().enumerate() {
        let nom = c.get("NAME").cloned().unwrap_or_default().to_ascii_lowercase();
        let type_odb = c.get("TYPE").cloned().unwrap_or_default();
        let contexte = c.get("CONTEXT").cloned().unwrap_or_default();
        let cote = if cuivres.is_empty() {
            Cote::Aucun
        } else if i <= premier {
            Cote::Dessus
        } else if i >= dernier {
            Cote::Dessous
        } else {
            Cote::Interne
        };
        let numero = cuivres.iter().position(|&k| k == i).map(|p| p as u32 + 1);
        let role = role_odb(&type_odb, &nom, cote, numero, cuivres.len());
        let mut dessin = Dessin::default();
        let mut nb = 0;
        let nets_couche = nets_par_element.get(&nom);
        if let Some(t) = arbre.texte(&format!("steps/{etape}/layers/{nom}/features")) {
            nb = dessiner_etape(&arbre, &etape, &nom, &mut dessin, &mut symboles, police.as_ref(), nets_couche, Transfo::IDENTITE, 0, Some(t));
        }
        let mut outils = Vec::new();
        if role.genre == Genre::Percage || role.genre == Genre::Fraisage {
            outils = lire_outils(&arbre, &etape, &nom, &dessin);
        }
        let mut role = role;
        if type_odb == "COMPONENT" {
            role = couches::Role { libelle: if nom.contains("bot") { "Composants dessous".into() } else { "Composants dessus".into() }, ..role };
        }
        couches_odb.push(CoucheOdb {
            nom: nom.clone(),
            type_odb,
            contexte,
            polarite_negative: c.get("POLARITY").map(|p| p == "NEGATIVE").unwrap_or(false),
            debut: c.get("START_NAME").cloned().unwrap_or_default().to_ascii_lowercase(),
            fin: c.get("END_NAME").cloned().unwrap_or_default().to_ascii_lowercase(),
            role,
            dessin,
            elements: nb,
            outils,
        });
    }

    // Composants et leurs boîtiers
    let boitiers = lire_boitiers(&arbre, &etape);
    let mut composants = Vec::new();
    for couche in couches_odb.iter_mut().filter(|c| c.type_odb == "COMPONENT") {
        let dessous = couche.nom.contains("bot");
        if let Some(t) = arbre.texte(&format!("steps/{etape}/layers/{}/components", couche.nom)) {
            let liste = lire_composants(&t, dessous, &boitiers);
            for comp in &liste {
                let info: Info = {
                    let mut v: Info = vec![
                        ("Objet".into(), "Composant".into()),
                        ("Référence".into(), comp.reference.clone()),
                        ("Pièce".into(), comp.piece.clone()),
                        ("Boîtier".into(), comp.boitier.clone()),
                        ("Position".into(), format!("X {} ; Y {} mm", nombre(comp.x), nombre(comp.y))),
                        ("Rotation".into(), format!("{}°", nombre(comp.rotation))),
                        ("Côté".into(), if dessous { "dessous".into() } else { "dessus".into() }),
                    ];
                    if !comp.valeur.is_empty() {
                        v.insert(3, ("Valeur".into(), comp.valeur.clone()));
                    }
                    v
                };
                let objet = couche.dessin.nouvel_objet(info);
                let t = placement(comp.x, comp.y, comp.rotation, dessous);
                match boitiers.iter().find(|b| b.nom == comp.boitier) {
                    Some(b) => {
                        for p in &b.contour {
                            couche.dessin.ajoute(p.transformee(&t), true, objet);
                        }
                        for p in &b.broches {
                            couche.dessin.ajoute(p.transformee(&t), true, objet);
                        }
                    }
                    None => couche.dessin.ajoute(Prim::Cap { a: pt(comp.x, comp.y), b: pt(comp.x, comp.y), r: 0.3 }, true, objet),
                }
            }
            couche.elements = liste.len();
            composants.extend(liste);
        }
    }

    // Contour de la carte (profil)
    if let Some(t) = arbre.texte(&format!("steps/{etape}/profile")) {
        let mut dessin = Dessin::default();
        let nb = dessiner_features(&t, &mut dessin, &mut symboles, police.as_ref(), None, &Transfo::IDENTITE, &unite_defaut, true);
        // Le profil est une surface : on n'en garde que le bord, en trait fin, comme une couche de contour.
        let mut bord = Dessin::default();
        let objet = bord.nouvel_objet(vec![("Objet".into(), "Contour de la carte (profil ODB++)".into())]);
        for e in &dessin.elements {
            if let Prim::Poly { anneaux } = &e.prim {
                for a in anneaux {
                    for k in 0..a.len() {
                        bord.ajoute(Prim::Cap { a: a[k], b: a[(k + 1) % a.len()], r: 0.0 }, true, objet);
                    }
                }
            }
        }
        if nb > 0 {
            couches_odb.push(CoucheOdb {
                nom: "profile".into(),
                type_odb: "PROFILE".into(),
                contexte: "BOARD".into(),
                polarite_negative: false,
                debut: String::new(),
                fin: String::new(),
                role: couches::Role { libelle: "Contour de la carte".into(), ..couches::depuis_x2(&["Profile".into()]).unwrap() },
                dessin: bord,
                elements: nb,
                outils: Vec::new(),
            });
        }
    }

    alertes.extend(symboles.alertes.drain(..));
    if police.is_none() {
        alertes.push("Police fonts/standard absente : les textes ne sont pas dessinés.".into());
    }
    Ok(Odb { info, etape, etapes, couches: couches_odb, composants, nets, alertes, unite: unite_defaut })
}

fn libelle_info(k: &str) -> String {
    match k {
        "JOB_NAME" => "Nom du produit",
        "ODB_VERSION_MAJOR" => "Version ODB++ (majeure)",
        "ODB_VERSION_MINOR" => "Version ODB++ (mineure)",
        "ODB_SOURCE" => "Logiciel d'origine",
        "CREATION_DATE" => "Date de création",
        "SAVE_DATE" => "Dernier enregistrement",
        "SAVE_APP" => "Enregistré avec",
        "SAVE_USER" => "Enregistré par",
        "UNITS" => "Unités",
        "MAX_UID" => "Identifiant maximal",
        _ => k,
    }
    .to_string()
}

fn formater_info(k: &str, v: &str) -> String {
    if k.ends_with("_DATE") && v.len() >= 8 {
        // 20161024.101454 -> 24/10/2016 10:14:54
        let d = &v[..8];
        let h = v.get(9..15).unwrap_or("");
        let mut s = format!("{}/{}/{}", &d[6..8], &d[4..6], &d[0..4]);
        if h.len() == 6 {
            s += &format!(" {}:{}:{}", &h[0..2], &h[2..4], &h[4..6]);
        }
        return s;
    }
    if k == "UNITS" {
        return if v.eq_ignore_ascii_case("MM") { "millimètres".into() } else { "pouces".into() };
    }
    v.to_string()
}

/// Blocs « NOM { CLE=VALEUR … } » (matrice, stephdr, tools).
fn blocs(t: &str) -> Vec<(String, BTreeMap<String, String>)> {
    let mut r = Vec::new();
    let mut courant: Option<(String, BTreeMap<String, String>)> = None;
    for l in t.lines() {
        let l = l.trim();
        if let Some(nom) = l.strip_suffix('{') {
            courant = Some((nom.trim().to_string(), BTreeMap::new()));
        } else if l == "}" {
            if let Some(b) = courant.take() {
                r.push(b);
            }
        } else if let Some((k, v)) = l.split_once('=') {
            if let Some((_, m)) = courant.as_mut() {
                m.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    r
}

fn est_cuivre(c: &BTreeMap<String, String>) -> bool {
    c.get("CONTEXT").map(|x| x == "BOARD").unwrap_or(false) && matches!(c.get("TYPE").map(|s| s.as_str()), Some("SIGNAL" | "POWER_GROUND" | "MIXED"))
}

fn role_odb(type_odb: &str, nom: &str, cote: Cote, numero: Option<u32>, nb_cuivres: usize) -> Role {
    let source = "ODB++";
    let mk = |g: Genre, c: Cote, n: Option<u32>| Role {
        genre: g,
        cote: c,
        numero: n,
        libelle: couches::nommer(g, c, n),
        couleur: couches::couleur(g, c, n),
        rang: 0,
        source,
    };
    let cote_ext = if cote == Cote::Interne { Cote::Aucun } else { cote };
    let mut r = match type_odb {
        "SIGNAL" | "POWER_GROUND" | "MIXED" => {
            let c = if numero == Some(1) { Cote::Dessus } else if numero == Some(nb_cuivres as u32) { Cote::Dessous } else { Cote::Interne };
            mk(Genre::Cuivre, c, numero)
        }
        "SOLDER_MASK" => mk(Genre::Masque, cote_ext, None),
        "SOLDER_PASTE" => mk(Genre::Pate, cote_ext, None),
        "SILK_SCREEN" => mk(Genre::Serigraphie, cote_ext, None),
        "DRILL" => mk(Genre::Percage, Cote::Aucun, None),
        "ROUT" => mk(Genre::Fraisage, Cote::Aucun, None),
        "COMPONENT" => mk(Genre::Composants, if nom.contains("bot") { Cote::Dessous } else { Cote::Dessus }, None),
        "DOCUMENT" => mk(Genre::Autre, Cote::Aucun, None),
        _ => mk(Genre::Autre, Cote::Aucun, None),
    };
    // Rang : on reprend celui des couches Gerber équivalentes.
    let reference = match r.genre {
        Genre::Cuivre if r.cote == Cote::Interne => couches::depuis_x2(&["Copper".into(), format!("L{}", numero.unwrap_or(2)), "Inr".into()]),
        Genre::Cuivre => couches::depuis_x2(&["Copper".into(), "L1".into(), if r.cote == Cote::Dessous { "Bot".into() } else { "Top".into() }]),
        Genre::Masque => couches::depuis_x2(&["Soldermask".into(), if r.cote == Cote::Dessous { "Bot".into() } else { "Top".into() }]),
        Genre::Pate => couches::depuis_x2(&["Paste".into(), if r.cote == Cote::Dessous { "Bot".into() } else { "Top".into() }]),
        Genre::Serigraphie => couches::depuis_x2(&["Legend".into(), if r.cote == Cote::Dessous { "Bot".into() } else { "Top".into() }]),
        Genre::Composants => couches::depuis_x2(&["Component".into(), "L1".into(), if r.cote == Cote::Dessous { "Bot".into() } else { "Top".into() }]),
        Genre::Percage => couches::depuis_x2(&["Plated".into()]),
        Genre::Fraisage => couches::depuis_x2(&["Plated".into(), "1".into(), "2".into(), "PTH".into(), "Rout".into()]),
        _ => None,
    };
    if let Some(x) = reference {
        r.rang = x.rang;
    } else {
        r.rang = 280;
    }
    if matches!(r.genre, Genre::Masque | Genre::Pate | Genre::Serigraphie) && r.cote == Cote::Aucun {
        r.libelle = format!("{} « {nom} »", r.libelle);
    }
    if r.genre == Genre::Autre || r.genre == Genre::Percage || r.genre == Genre::Fraisage {
        r.libelle = match r.genre {
            Genre::Percage => format!("Perçage « {nom} »"),
            Genre::Fraisage => format!("Fraisage « {nom} »"),
            _ => nom.to_string(),
        };
    }
    r
}

fn choisir_etape(arbre: &Arbre, etapes: &[String]) -> String {
    if etapes.len() <= 1 {
        return etapes.first().cloned().unwrap_or_else(|| "pcb".into());
    }
    let repete: Vec<String> = etapes
        .iter()
        .flat_map(|e| arbre.texte(&format!("steps/{e}/stephdr")).map(|t| blocs(&t).into_iter().filter(|(n, _)| n == "STEP-REPEAT").filter_map(|(_, c)| c.get("NAME").map(|s| s.to_ascii_lowercase())).collect::<Vec<_>>()).unwrap_or_default())
        .collect();
    // Une carte unique : une étape répétée par une autre (le circuit dans son panneau), sinon une étape « pcb ».
    if let Some(e) = etapes.iter().find(|e| repete.contains(e)) {
        return e.clone();
    }
    for motif in ["pcb", "board", "edit", "orig"] {
        if let Some(e) = etapes.iter().find(|e| e.contains(motif)) {
            return e.clone();
        }
    }
    etapes[0].clone()
}

/// Dessine une couche d'une étape (avec ses répétitions éventuelles). Renvoie le nombre d'éléments lus.
#[allow(clippy::too_many_arguments)]
fn dessiner_etape(arbre: &Arbre, etape: &str, couche: &str, dessin: &mut Dessin, symboles: &mut Symboles, police: Option<&Police>, nets: Option<&HashMap<usize, String>>, t: Transfo, profondeur: u32, texte: Option<String>) -> usize {
    let mut nb = 0;
    let texte = texte.or_else(|| arbre.texte(&format!("steps/{etape}/layers/{couche}/features")));
    if let Some(texte) = texte {
        nb += dessiner_features(&texte, dessin, symboles, police, nets, &t, "INCH", false);
    }
    if profondeur < 4 {
        if let Some(h) = arbre.texte(&format!("steps/{etape}/stephdr")) {
            let unite_mm = h.lines().any(|l| l.trim().eq_ignore_ascii_case("UNITS=MM"));
            let k = if unite_mm { 1.0 } else { 25.4 };
            for (n, c) in blocs(&h) {
                if n != "STEP-REPEAT" {
                    continue;
                }
                let enfant = c.get("NAME").cloned().unwrap_or_default().to_ascii_lowercase();
                let g = |cle: &str| c.get(cle).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
                let (nx, ny) = (g("NX").max(1.0) as usize, g("NY").max(1.0) as usize);
                for j in 0..ny.min(200) {
                    for i in 0..nx.min(200) {
                        let tt = t
                            .apres(&Transfo::translation((g("X") + i as f64 * g("DX")) * k, (g("Y") + j as f64 * g("DY")) * k))
                            .apres(&Transfo::rotation(-g("ANGLE").to_radians()))
                            .apres(&Transfo::miroir(c.get("MIRROR").map(|m| m == "YES").unwrap_or(false), false));
                        nb += dessiner_etape(arbre, &enfant, couche, dessin, symboles, police, None, tt, profondeur + 1, None);
                    }
                }
            }
        }
    }
    nb
}

/// Correspondance numéro d'orientation -> (angle horaire en degrés, miroir).
fn orientation(champs: &[&str], i: &mut usize) -> (f64, bool) {
    let o: i32 = champs.get(*i).and_then(|v| v.parse().ok()).unwrap_or(0);
    *i += 1;
    match o {
        0..=3 => (o as f64 * 90.0, false),
        4..=7 => ((o - 4) as f64 * 90.0, true),
        8 | 9 => {
            let a: f64 = champs.get(*i).and_then(|v| v.parse().ok()).unwrap_or(0.0);
            *i += 1;
            (a, o == 9)
        }
        _ => (0.0, false),
    }
}

/// Placement d'un symbole : miroir (x → −x), puis rotation horaire, puis translation.
fn placement(x: f64, y: f64, angle_horaire: f64, miroir: bool) -> Transfo {
    Transfo::translation(x, y).apres(&Transfo::rotation(-angle_horaire.to_radians())).apres(&Transfo::miroir(miroir, false))
}

/// Lit un fichier « features » et le dessine. Renvoie le nombre d'éléments.
#[allow(clippy::too_many_arguments)]
fn dessiner_features(texte: &str, dessin: &mut Dessin, symboles: &mut Symboles, police: Option<&Police>, nets: Option<&HashMap<usize, String>>, t: &Transfo, unite_defaut: &str, profil: bool) -> usize {
    let mut unite_mm = unite_defaut.eq_ignore_ascii_case("MM");
    let mut table: Vec<(String, Option<bool>)> = Vec::new();
    let mut attributs: HashMap<usize, String> = HashMap::new();
    let mut textes: HashMap<usize, String> = HashMap::new();
    let mut index = 0usize;
    let mut lignes = texte.lines();
    let mut surface: Option<Surface> = None;
    while let Some(ligne) = lignes.next() {
        let l = ligne.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some(s) = surface.as_mut() {
            let c: Vec<&str> = l.split_whitespace().collect();
            let k = if unite_mm { 1.0 } else { 25.4 };
            let p = |a: usize, b: usize| pt(c.get(a).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) * k, c.get(b).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) * k);
            match c.first().copied() {
                Some("OB") => {
                    s.contour = vec![p(1, 2)];
                    s.trou = c.get(3).map(|v| *v == "H").unwrap_or(false);
                }
                Some("OS") => s.contour.push(p(1, 2)),
                Some("OC") => {
                    let fin = p(1, 2);
                    let centre = p(3, 4);
                    let horaire = c.get(5).map(|v| v.eq_ignore_ascii_case("Y")).unwrap_or(true);
                    let debut = *s.contour.last().unwrap_or(&fin);
                    let a = arc(debut, fin, centre, horaire);
                    s.contour.extend_from_slice(&a[1..]);
                }
                Some("OE") => {
                    let contour = std::mem::take(&mut s.contour);
                    if s.trou {
                        if let Some(dernier) = s.iles.last_mut() {
                            dernier.push(contour);
                        }
                    } else {
                        s.iles.push(vec![contour]);
                    }
                }
                Some("SE") => {
                    let s = surface.take().unwrap();
                    for ile in s.iles {
                        dessin.ajoute(Prim::Poly { anneaux: ile }.transformee(t), s.sombre, s.objet);
                    }
                }
                _ => {}
            }
            continue;
        }
        if let Some(v) = l.strip_prefix("UNITS=") {
            unite_mm = v.trim().eq_ignore_ascii_case("MM");
            continue;
        }
        if let Some(r) = l.strip_prefix('$') {
            let mut p = r.split_whitespace();
            let n: usize = p.next().and_then(|v| v.parse().ok()).unwrap_or(table.len());
            let nom = p.next().unwrap_or("").to_string();
            let unite = p.next().map(|u| u.eq_ignore_ascii_case("M"));
            if table.len() <= n {
                table.resize(n + 1, (String::new(), None));
            }
            table[n] = (nom, unite);
            continue;
        }
        if let Some(r) = l.strip_prefix('@') {
            if let Some((n, nom)) = r.split_once(' ') {
                attributs.insert(n.parse().unwrap_or(0), nom.trim().to_string());
            }
            continue;
        }
        if let Some(r) = l.strip_prefix('&') {
            if let Some((n, v)) = r.split_once(' ') {
                textes.insert(n.parse().unwrap_or(0), v.trim().to_string());
            }
            continue;
        }
        let (corps, attrs) = match l.find(';') {
            Some(p) => (&l[..p], &l[p + 1..]),
            None => (l, ""),
        };
        let c: Vec<&str> = corps.split_whitespace().collect();
        let Some(&code) = c.first() else { continue };
        if !matches!(code, "P" | "L" | "A" | "S" | "T" | "B") {
            continue;
        }
        let k = if unite_mm { 1.0 } else { 25.4 };
        let f = |i: usize| c.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        let mut info: Info = vec![("Objet".into(), match code {
            "P" => "Pastille",
            "L" => "Trait",
            "A" => "Arc",
            "S" => "Surface",
            "T" => "Texte",
            _ => "Code-barres",
        }.into())];
        if profil {
            info = vec![("Objet".into(), "Profil".into())];
        }
        let symbole_info = |n: usize| -> String { table.get(n).map(|s| s.0.clone()).unwrap_or_default() };
        let net = nets.and_then(|m| m.get(&index)).cloned();
        let idx = index;
        index += 1;
        match code {
            "P" => {
                let mut i = 3;
                let mut sym: i64 = c.get(i).and_then(|v| v.parse().ok()).unwrap_or(0);
                i += 1;
                if sym == -1 {
                    sym = c.get(i).and_then(|v| v.parse().ok()).unwrap_or(0);
                    i += 2;
                }
                let sombre = c.get(i).map(|v| *v != "N").unwrap_or(true);
                i += 2; // polarité, dcode
                let (angle, miroir) = orientation(&c, &mut i);
                info.push(("Symbole".into(), symbole_info(sym as usize)));
                ajouter_attributs(&mut info, attrs, &attributs, &textes, net.as_deref());
                let objet = dessin.nouvel_objet(info);
                let tt = t.apres(&placement(f(1) * k, f(2) * k, angle, miroir));
                let (nom, u) = table.get(sym as usize).cloned().unwrap_or_default();
                for (p, expose) in symboles.forme(&nom, u.unwrap_or(unite_mm)) {
                    dessin.ajoute(p.transformee(&tt), if expose { sombre } else { !sombre }, objet);
                }
            }
            "L" | "A" => {
                let (debut, fin) = (pt(f(1) * k, f(2) * k), pt(f(3) * k, f(4) * k));
                let (sym_i, pol_i) = if code == "L" { (5, 6) } else { (7, 8) };
                let sym = c.get(sym_i).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
                let sombre = c.get(pol_i).map(|v| *v != "N").unwrap_or(true);
                let (nom, u) = table.get(sym).cloned().unwrap_or_default();
                info.push(("Symbole".into(), nom.clone()));
                ajouter_attributs(&mut info, attrs, &attributs, &textes, net.as_deref());
                let objet = dessin.nouvel_objet(info);
                let points = if code == "A" {
                    let centre = pt(f(5) * k, f(6) * k);
                    let horaire = c.get(10).map(|v| v.eq_ignore_ascii_case("Y")).unwrap_or(true);
                    arc(debut, fin, centre, horaire)
                } else {
                    vec![debut, fin]
                };
                let (largeur, carre) = largeur_trait(&nom, u.unwrap_or(unite_mm));
                for w in points.windows(2) {
                    let prim = if carre {
                        let h = largeur / 2.0;
                        let mut coins = Vec::new();
                        for p in w {
                            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                                coins.push(pt(p.x + sx * h, p.y + sy * h));
                            }
                        }
                        Prim::Poly { anneaux: vec![crate::gerber::enveloppe_convexe(coins)] }
                    } else {
                        Prim::Cap { a: w[0], b: w[1], r: largeur / 2.0 }
                    };
                    dessin.ajoute(prim.transformee(t), sombre, objet);
                }
            }
            "S" => {
                let sombre = c.get(1).map(|v| *v != "N").unwrap_or(true);
                ajouter_attributs(&mut info, attrs, &attributs, &textes, net.as_deref());
                let objet = dessin.nouvel_objet(info);
                surface = Some(Surface { sombre, objet, iles: Vec::new(), contour: Vec::new(), trou: false });
            }
            "T" => {
                // T x y font polarité orient… xsize ysize width_factor 'texte' version
                let sombre = c.get(4).map(|v| *v != "N").unwrap_or(true);
                let mut i = 5;
                let (angle, miroir) = orientation(&c, &mut i);
                let (xs, ys) = (f(i) * k, f(i + 1) * k);
                let largeur = f(i + 2);
                let texte = corps.find('\'').and_then(|d| corps.rfind('\'').filter(|&e| e > d).map(|e| corps[d + 1..e].to_string())).unwrap_or_default();
                info.push(("Texte".into(), texte.clone()));
                let objet = dessin.nouvel_objet(info);
                if let Some(p) = police {
                    let tt = t.apres(&placement(f(1) * k, f(2) * k, angle, miroir));
                    for prim in p.dessiner(&texte, xs, ys, largeur) {
                        dessin.ajoute(prim.transformee(&tt), sombre, objet);
                    }
                }
            }
            _ => {
                let _ = idx;
            }
        }
    }
    index
}

struct Surface {
    sombre: bool,
    objet: u32,
    iles: Vec<Vec<Vec<Pt>>>,
    contour: Vec<Pt>,
    trou: bool,
}

fn ajouter_attributs(info: &mut Info, attrs: &str, noms: &HashMap<usize, String>, textes: &HashMap<usize, String>, net: Option<&str>) {
    if let Some(n) = net {
        if n != "$NONE$" {
            info.push(("Net".into(), n.to_string()));
        }
    }
    let premier = attrs.split(';').next().unwrap_or("");
    for a in premier.split(',') {
        let a = a.trim();
        if a.is_empty() || a.starts_with("ID=") {
            continue;
        }
        let (n, v) = match a.split_once('=') {
            Some((n, v)) => (n, Some(v)),
            None => (a, None),
        };
        let Some(nom) = n.parse::<usize>().ok().and_then(|i| noms.get(&i)) else { continue };
        let valeur = match v {
            Some(v) if nom == ".string" || nom.ends_with("_name") || nom == ".comp_name" => v.parse::<usize>().ok().and_then(|i| textes.get(&i).cloned()).unwrap_or(v.to_string()),
            Some(v) => v.to_string(),
            None => "oui".into(),
        };
        let libelle = match nom.as_str() {
            ".pad_usage" => "Usage de la pastille".to_string(),
            ".smd" => "CMS".to_string(),
            ".string" => "Texte".to_string(),
            ".geometry" => "Géométrie".to_string(),
            ".test_point" => "Point de test".to_string(),
            ".drill" => "Perçage".to_string(),
            ".nomenclature" => "Nomenclature".to_string(),
            autre => autre.trim_start_matches('.').to_string(),
        };
        let valeur = if nom == ".pad_usage" {
            match valeur.as_str() {
                "0" => "connexion (toeprint)".into(),
                "1" => "via".into(),
                "2" => "pastille de repère (fiducial)".into(),
                "3" => "pastille isolée".into(),
                "4" => "pastille sans usage".into(),
                _ => valeur,
            }
        } else {
            valeur
        };
        info.push((libelle, valeur));
    }
}

/// Largeur d'un trait d'après son symbole (rond ou carré), en mm.
fn largeur_trait(nom: &str, unite_mm: bool) -> (f64, bool) {
    let k = if unite_mm { 0.001 } else { 0.0254 };
    if let Some(v) = nom.strip_prefix('r').and_then(|v| v.parse::<f64>().ok()) {
        return (v * k, false);
    }
    if let Some(v) = nom.strip_prefix('s').and_then(|v| v.parse::<f64>().ok()) {
        return (v * k, true);
    }
    // Autre symbole utilisé en trait : on prend sa plus petite dimension.
    let nombres: Vec<f64> = nom.split(|c: char| !(c.is_ascii_digit() || c == '.')).filter_map(|v| v.parse().ok()).collect();
    (nombres.iter().cloned().fold(f64::INFINITY, f64::min).min(1000.0).max(0.0) * k, false)
}

struct Symboles<'a, 'b> {
    arbre: &'b Arbre<'a>,
    cache: HashMap<(String, bool), Vec<(Prim, bool)>>,
    alertes: Vec<String>,
}

impl Symboles<'_, '_> {
    fn forme(&mut self, nom: &str, unite_mm: bool) -> Vec<(Prim, bool)> {
        let cle = (nom.to_string(), unite_mm);
        if let Some(f) = self.cache.get(&cle) {
            return f.clone();
        }
        let k = if unite_mm { 0.001 } else { 0.0254 };
        let forme = match symbole_standard(nom, k) {
            Some(f) => f,
            None => {
                // Symbole défini par l'utilisateur : symbols/<nom>/features
                match self.arbre.texte(&format!("symbols/{}/features", nom.to_ascii_lowercase())) {
                    Some(t) => {
                        let mut d = Dessin::default();
                        let police = None;
                        let mut sous = Symboles { arbre: self.arbre, cache: HashMap::new(), alertes: Vec::new() };
                        dessiner_features(&t, &mut d, &mut sous, police, None, &Transfo::IDENTITE, if unite_mm { "MM" } else { "INCH" }, false);
                        self.alertes.extend(sous.alertes);
                        d.elements.into_iter().map(|e| (e.prim, e.sombre)).collect()
                    }
                    None => {
                        let m = format!("Symbole « {nom} » inconnu : dessiné comme un petit disque.");
                        if self.alertes.len() < 20 && !self.alertes.contains(&m) {
                            self.alertes.push(m);
                        }
                        vec![(Prim::Cap { a: pt(0.0, 0.0), b: pt(0.0, 0.0), r: 0.1 }, true)]
                    }
                }
            }
        };
        // Parties négatives d'un symbole : trous transparents, comme pour les macros Gerber.
        let forme: Vec<(Prim, bool)> = if forme.iter().any(|(_, e)| !e) { forme_booleenne(&forme).into_iter().map(|p| (p, true)).collect() } else { forme };
        self.cache.insert(cle, forme.clone());
        forme
    }
}

/// Symboles standard ODB++ (dimensions en mils pour les fichiers en pouces, en microns pour les fichiers en mm).
fn symbole_standard(nom: &str, k: f64) -> Option<Vec<(Prim, bool)>> {
    let o = pt(0.0, 0.0);
    let nombres = |s: &str| -> Vec<String> { s.split('x').map(|v| v.to_string()).collect() };
    let val = |v: &str| v.parse::<f64>().ok().map(|x| x * k);
    let poly = |pts: Vec<Pt>| (Prim::Poly { anneaux: vec![pts] }, true);
    let prefixes = ["donut_rc", "donut_r", "donut_s", "donut_sr", "rc_ths", "rc_tho", "s_ths", "s_tho", "sr_ths", "oval_h", "hex_l", "hex_s", "moire", "rect", "oval", "null", "hole", "thr", "ths", "oct", "bfr", "bfs", "tri", "di", "el", "r", "s"];
    let prefixe = prefixes.iter().find(|p| nom.starts_with(**p) && nom[p.len()..].chars().next().map(|c| c.is_ascii_digit() || c == '.').unwrap_or(false))?;
    let reste = &nom[prefixe.len()..];
    let p = nombres(reste);
    let n = |i: usize| p.get(i).and_then(|v| val(v));
    Some(match *prefixe {
        "r" => vec![(Prim::Cap { a: o, b: o, r: n(0)? / 2.0 }, true)],
        "s" => vec![(Prim::Boite { c: o, hw: n(0)? / 2.0, hh: n(0)? / 2.0, ang: 0.0, rad: 0.0 }, true)],
        "rect" => {
            let (w, h) = (n(0)?, n(1)?);
            let mut rad = 0.0;
            let mut chanfrein = false;
            let mut coins = 0b1111u8;
            for x in p.iter().skip(2) {
                if let Some(v) = x.strip_prefix('r') {
                    rad = val(v).unwrap_or(0.0);
                } else if let Some(v) = x.strip_prefix('c') {
                    rad = val(v).unwrap_or(0.0);
                    chanfrein = true;
                } else if !x.is_empty() && x.chars().all(|c| ('1'..='4').contains(&c)) {
                    coins = x.chars().fold(0, |m, c| m | 1 << (c as u8 - b'1'));
                }
            }
            if rad > 0.0 && (chanfrein || coins != 0b1111) {
                vec![poly(rectangle_coins(o, w, h, rad, chanfrein, coins))]
            } else {
                vec![(Prim::Boite { c: o, hw: w / 2.0, hh: h / 2.0, ang: 0.0, rad }, true)]
            }
        }
        "oval" => {
            let (w, h) = (n(0)?, n(1)?);
            vec![(Prim::Boite { c: o, hw: w / 2.0, hh: h / 2.0, ang: 0.0, rad: w.min(h) / 2.0 }, true)]
        }
        "di" => {
            let (w, h) = (n(0)?, n(1)?);
            vec![poly(vec![pt(w / 2.0, 0.0), pt(0.0, h / 2.0), pt(-w / 2.0, 0.0), pt(0.0, -h / 2.0)])]
        }
        "oct" => {
            let (w, h, r) = (n(0)?, n(1)?, n(2)?);
            vec![poly(rectangle_coins(o, w, h, r, true, 0b1111))]
        }
        "donut_r" => {
            let (d, i) = (n(0)?, n(1)?);
            vec![(Prim::Poly { anneaux: vec![cercle(o, d / 2.0), cercle(o, i / 2.0)] }, true)]
        }
        "donut_s" => {
            let (d, i) = (n(0)?, n(1)?);
            vec![(Prim::Poly { anneaux: vec![rectangle_coins(o, d, d, 0.0, false, 0), rectangle_coins(o, i, i, 0.0, false, 0)] }, true)]
        }
        "donut_sr" => {
            let (d, i) = (n(0)?, n(1)?);
            vec![(Prim::Poly { anneaux: vec![rectangle_coins(o, d, d, 0.0, false, 0), cercle(o, i / 2.0)] }, true)]
        }
        "donut_rc" => {
            let (w, h, lw) = (n(0)?, n(1)?, n(2)?);
            vec![(Prim::Poly { anneaux: vec![rectangle_coins(o, w, h, 0.0, false, 0), rectangle_coins(o, w - 2.0 * lw, h - 2.0 * lw, 0.0, false, 0)] }, true)]
        }
        "hex_l" => {
            let (w, h, r) = (n(0)?, n(1)?, n(2)?);
            vec![poly(vec![pt(-w / 2.0, 0.0), pt(-w / 2.0 + r, h / 2.0), pt(w / 2.0 - r, h / 2.0), pt(w / 2.0, 0.0), pt(w / 2.0 - r, -h / 2.0), pt(-w / 2.0 + r, -h / 2.0)])]
        }
        "hex_s" => {
            let (w, h, r) = (n(0)?, n(1)?, n(2)?);
            vec![poly(vec![pt(0.0, -h / 2.0), pt(w / 2.0, -h / 2.0 + r), pt(w / 2.0, h / 2.0 - r), pt(0.0, h / 2.0), pt(-w / 2.0, h / 2.0 - r), pt(-w / 2.0, -h / 2.0 + r)])]
        }
        "bfr" => {
            let d = n(0)?;
            vec![poly(secteur(d / 2.0, 0.0, PI / 2.0)), poly(secteur(d / 2.0, PI, 1.5 * PI))]
        }
        "bfs" => {
            let s = n(0)?;
            vec![poly(vec![o, pt(s / 2.0, 0.0), pt(s / 2.0, s / 2.0), pt(0.0, s / 2.0)]), poly(vec![o, pt(-s / 2.0, 0.0), pt(-s / 2.0, -s / 2.0), pt(0.0, -s / 2.0)])]
        }
        "tri" => {
            let (b, h) = (n(0)?, n(1)?);
            vec![poly(vec![pt(-b / 2.0, -h / 2.0), pt(b / 2.0, -h / 2.0), pt(0.0, h / 2.0)])]
        }
        "oval_h" => {
            let (w, h) = (n(0)?, n(1)?);
            let r = h / 2.0;
            let mut pts = vec![pt(-w / 2.0, -r), pt(w / 2.0 - r, -r)];
            pts.extend(arc(pt(w / 2.0 - r, -r), pt(w / 2.0 - r, r), pt(w / 2.0 - r, 0.0), false));
            pts.push(pt(-w / 2.0, r));
            vec![poly(pts)]
        }
        "el" => {
            let (w, h) = (n(0)?, n(1)?);
            let m = segments_cercle(w.max(h) / 2.0, 2.0 * PI);
            vec![poly((0..m).map(|i| {
                let a = 2.0 * PI * i as f64 / m as f64;
                pt(w / 2.0 * a.cos(), h / 2.0 * a.sin())
            }).collect())]
        }
        "thr" | "ths" => {
            let (d, i) = (n(0)?, n(1)?);
            let angle = p.get(2).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0).to_radians();
            let branches = p.get(3).and_then(|v| v.parse::<f64>().ok()).unwrap_or(4.0) as usize;
            let ecart = n(4).unwrap_or(0.0);
            let ext = if *prefixe == "ths" { rectangle_coins(o, d, d, 0.0, false, 0) } else { cercle(o, d / 2.0) };
            thermique(o, &ext, &cercle(o, i / 2.0), branches, angle, ecart).into_iter().map(|p| (p, true)).collect()
        }
        "s_ths" | "s_tho" | "sr_ths" => {
            let (d, i) = (n(0)?, n(1)?);
            let angle = p.get(2).and_then(|v| v.parse::<f64>().ok()).unwrap_or(45.0).to_radians();
            let branches = p.get(3).and_then(|v| v.parse::<f64>().ok()).unwrap_or(4.0) as usize;
            let ecart = n(4).unwrap_or(0.0);
            let int = if *prefixe == "sr_ths" { cercle(o, i / 2.0) } else { rectangle_coins(o, i, i, 0.0, false, 0) };
            thermique(o, &rectangle_coins(o, d, d, 0.0, false, 0), &int, branches, angle, ecart).into_iter().map(|p| (p, true)).collect()
        }
        "rc_ths" | "rc_tho" => {
            let (w, h) = (n(0)?, n(1)?);
            let angle = p.get(2).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0).to_radians();
            let branches = p.get(3).and_then(|v| v.parse::<f64>().ok()).unwrap_or(4.0) as usize;
            let ecart = n(4).unwrap_or(0.0);
            let anneau = n(5).unwrap_or(w.min(h) / 4.0);
            thermique(o, &rectangle_coins(o, w, h, 0.0, false, 0), &rectangle_coins(o, w - 2.0 * anneau, h - 2.0 * anneau, 0.0, false, 0), branches, angle, ecart).into_iter().map(|p| (p, true)).collect()
        }
        "moire" => {
            let (rw, rg, nr, lw, ll) = (n(0)?, n(1)?, p.get(2).and_then(|v| v.parse::<f64>().ok()).unwrap_or(1.0) as usize, n(3)?, n(4)?);
            let angle = p.get(5).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0).to_radians();
            let mut r = Vec::new();
            for j in 0..nr.max(1) {
                let ext = rg * (j + 1) as f64 + rw * (j + 1) as f64;
                let int = ext - rw;
                r.push((Prim::Poly { anneaux: vec![cercle(o, ext), cercle(o, int.max(0.0))] }, true));
            }
            let t = Transfo::rotation(angle);
            r.push((Prim::Boite { c: o, hw: ll / 2.0, hh: lw / 2.0, ang: 0.0, rad: 0.0 }.transformee(&t), true));
            r.push((Prim::Boite { c: o, hw: lw / 2.0, hh: ll / 2.0, ang: 0.0, rad: 0.0 }.transformee(&t), true));
            r
        }
        "hole" => vec![(Prim::Cap { a: o, b: o, r: n(0)? / 2.0 }, true)],
        "null" => Vec::new(),
        _ => return None,
    })
}

fn secteur(r: f64, a0: f64, a1: f64) -> Vec<Pt> {
    let o = pt(0.0, 0.0);
    let mut pts = vec![o];
    pts.extend(arc(pt(r * a0.cos(), r * a0.sin()), pt(r * a1.cos(), r * a1.sin()), o, false));
    pts
}

/// Police vectorielle ODB++ (fonts/standard) : chaque caractère est une suite de traits.
struct Police {
    largeur: f64,
    hauteur: f64,
    ecart: f64,
    car: HashMap<char, Vec<(Pt, Pt, f64)>>,
}

impl Police {
    fn lire(t: &str) -> Police {
        let mut p = Police { largeur: 0.3, hauteur: 0.3, ecart: 0.0, car: HashMap::new() };
        let mut courant: Option<char> = None;
        for l in t.lines() {
            let c: Vec<&str> = l.split_whitespace().collect();
            let f = |i: usize| c.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
            match c.first().copied() {
                Some("XSIZE") => p.largeur = f(1),
                Some("YSIZE") => p.hauteur = f(1),
                Some("OFFSET") => p.ecart = f(1),
                Some("CHAR") => courant = l.trim_start()[4..].trim_start().chars().next().or(Some(' ')),
                Some("ECHAR") => courant = None,
                Some("LINE") => {
                    if let Some(ch) = courant {
                        p.car.entry(ch).or_default().push((pt(f(1), f(2)), pt(f(3), f(4)), f(7)));
                    }
                }
                _ => {}
            }
        }
        p
    }

    /// Traits du texte, en mm, à partir de l'origine (coin inférieur gauche). xs, ys : taille d'un caractère (mm) ;
    /// facteur : épaisseur relative.
    fn dessiner(&self, texte: &str, xs: f64, ys: f64, facteur: f64) -> Vec<Prim> {
        let sx = if self.largeur > 0.0 { xs / self.largeur } else { 1.0 };
        let sy = if self.hauteur > 0.0 { ys / self.hauteur } else { 1.0 };
        let mut r = Vec::new();
        let mut x = 0.0;
        for ch in texte.chars() {
            if let Some(traits) = self.car.get(&ch).or_else(|| self.car.get(&ch.to_ascii_uppercase())) {
                for (a, b, l) in traits {
                    let w = (l * 25.4 * facteur.max(0.1)).max(0.0) * sx.min(sy).min(1.0).max(0.2);
                    r.push(Prim::Cap { a: pt(x + a.x * 25.4 * sx, a.y * 25.4 * sy), b: pt(x + b.x * 25.4 * sx, b.y * 25.4 * sy), r: w / 2.0 });
                }
            }
            x += xs + self.ecart * 25.4 * sx;
        }
        r
    }
}

/// Nets de eda/data : liste (nom, éléments) et, pour chaque couche, élément -> net.
fn lire_nets(arbre: &Arbre, etape: &str) -> (Vec<(String, usize)>, HashMap<String, HashMap<usize, String>>) {
    let mut nets = Vec::new();
    let mut par_couche: HashMap<String, HashMap<usize, String>> = HashMap::new();
    let Some(t) = arbre.texte(&format!("steps/{etape}/eda/data")) else {
        if let Some(n) = arbre.texte(&format!("steps/{etape}/netlists/cadnet/netlist")) {
            for l in n.lines() {
                if let Some(r) = l.strip_prefix('$') {
                    if let Some((_, nom)) = r.split_once(' ') {
                        nets.push((nom.trim().to_string(), 0));
                    }
                }
            }
        }
        return (nets, par_couche);
    };
    let mut couches_eda: Vec<String> = Vec::new();
    let mut courant: Option<String> = None;
    let mut compte = 0usize;
    for l in t.lines() {
        let l = l.trim();
        if let Some(r) = l.strip_prefix("LYR ") {
            couches_eda = r.split_whitespace().map(|s| s.to_ascii_lowercase()).collect();
        } else if let Some(r) = l.strip_prefix("NET ") {
            if let Some(n) = courant.take() {
                nets.push((n, compte));
            }
            courant = Some(r.split(';').next().unwrap_or("").trim().to_string());
            compte = 0;
        } else if let Some(r) = l.strip_prefix("FID ") {
            let c: Vec<&str> = r.split_whitespace().collect();
            if let (Some(n), Some(couche), Some(element)) = (courant.as_ref(), c.get(1).and_then(|v| v.parse::<usize>().ok()), c.get(2).and_then(|v| v.parse::<usize>().ok())) {
                if let Some(nom) = couches_eda.get(couche) {
                    par_couche.entry(nom.clone()).or_default().insert(element, n.clone());
                }
                compte += 1;
            }
        } else if l.starts_with("PKG ") {
            if let Some(n) = courant.take() {
                nets.push((n, compte));
            }
            compte = 0;
        }
    }
    if let Some(n) = courant.take() {
        nets.push((n, compte));
    }
    nets.retain(|(n, _)| n != "$NONE$");
    (nets, par_couche)
}

struct Boitier {
    nom: String,
    contour: Vec<Prim>,
    broches: Vec<Prim>,
}

/// Boîtiers (PKG) de eda/data : contour du corps et forme des broches, en mm, dans le repère du boîtier.
fn lire_boitiers(arbre: &Arbre, etape: &str) -> Vec<Boitier> {
    let mut r: Vec<Boitier> = Vec::new();
    let Some(t) = arbre.texte(&format!("steps/{etape}/eda/data")) else { return r };
    let mut k = 25.4;
    let mut dans_broche = false;
    let mut contour: Vec<Pt> = Vec::new();
    for l in t.lines() {
        let l = l.trim();
        let c: Vec<&str> = l.split_whitespace().collect();
        let f = |i: usize| c.get(i).and_then(|v| v.trim_end_matches(';').parse::<f64>().ok()).unwrap_or(0.0) * k;
        match c.first().copied() {
            Some(u) if u.starts_with("UNITS=") => k = if u.eq_ignore_ascii_case("UNITS=MM") { 1.0 } else { 25.4 },
            Some("PKG") => {
                r.push(Boitier { nom: c.get(1).unwrap_or(&"").to_string(), contour: Vec::new(), broches: Vec::new() });
                dans_broche = false;
            }
            Some("PIN") => dans_broche = true,
            Some("NET") | Some("FGR") => dans_broche = false,
            Some("RC") => {
                if let Some(b) = r.last_mut() {
                    let (x, y, w, h) = (f(1), f(2), f(3), f(4));
                    let p = Prim::Poly { anneaux: vec![vec![pt(x, y), pt(x + w, y), pt(x + w, y + h), pt(x, y + h)]] };
                    if dans_broche { b.broches.push(p) } else { b.contour.extend(bord(&[pt(x, y), pt(x + w, y), pt(x + w, y + h), pt(x, y + h)])) }
                }
            }
            Some("CR") => {
                if let Some(b) = r.last_mut() {
                    let c0 = pt(f(1), f(2));
                    if dans_broche { b.broches.push(Prim::Cap { a: c0, b: c0, r: f(3) }) } else { b.contour.extend(bord(&cercle(c0, f(3)))) }
                }
            }
            Some("SQ") => {
                if let Some(b) = r.last_mut() {
                    let (cx, cy, h) = (f(1), f(2), f(3));
                    let pts = vec![pt(cx - h, cy - h), pt(cx + h, cy - h), pt(cx + h, cy + h), pt(cx - h, cy + h)];
                    if dans_broche { b.broches.push(Prim::Poly { anneaux: vec![pts] }) } else { b.contour.extend(bord(&pts)) }
                }
            }
            Some("CT") => contour.clear(),
            Some("OB") => contour = vec![pt(f(1), f(2))],
            Some("OS") => contour.push(pt(f(1), f(2))),
            Some("OC") => {
                let fin = pt(f(1), f(2));
                let debut = *contour.last().unwrap_or(&fin);
                let a = arc(debut, fin, pt(f(3), f(4)), c.get(5).map(|v| v.eq_ignore_ascii_case("Y")).unwrap_or(true));
                contour.extend_from_slice(&a[1..]);
            }
            Some("OE") => {
                if let Some(b) = r.last_mut() {
                    let pts = std::mem::take(&mut contour);
                    if pts.len() >= 3 {
                        if dans_broche { b.broches.push(Prim::Poly { anneaux: vec![pts] }) } else { b.contour.extend(bord(&pts)) }
                    }
                }
            }
            _ => {}
        }
    }
    r
}

/// Bord d'un polygone en traits fins (contour du corps d'un composant).
fn bord(pts: &[Pt]) -> Vec<Prim> {
    (0..pts.len()).map(|i| Prim::Cap { a: pts[i], b: pts[(i + 1) % pts.len()], r: 0.0 }).collect()
}

fn lire_composants(t: &str, dessous: bool, boitiers: &[Boitier]) -> Vec<Composant> {
    let mut r: Vec<Composant> = Vec::new();
    let mut k = 25.4;
    let mut attributs: HashMap<usize, String> = HashMap::new();
    let mut textes: HashMap<usize, String> = HashMap::new();
    for l in t.lines() {
        let l = l.trim();
        if let Some(u) = l.strip_prefix("UNITS=") {
            k = if u.trim().eq_ignore_ascii_case("MM") { 1.0 } else { 25.4 };
        } else if let Some(x) = l.strip_prefix('@') {
            if let Some((n, v)) = x.split_once(' ') {
                attributs.insert(n.parse().unwrap_or(0), v.trim().to_string());
            }
        } else if let Some(x) = l.strip_prefix('&') {
            if let Some((n, v)) = x.split_once(' ') {
                textes.insert(n.parse().unwrap_or(0), v.trim().to_string());
            }
        } else if let Some(x) = l.strip_prefix("CMP ") {
            let (corps, attrs) = x.split_once(';').unwrap_or((x, ""));
            let c: Vec<&str> = corps.split_whitespace().collect();
            let f = |i: usize| c.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
            let pkg = c.first().and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            let mut proprietes = Vec::new();
            for a in attrs.split(';').next().unwrap_or("").split(',') {
                if let Some((n, v)) = a.split_once('=') {
                    if let Some(nom) = n.trim().parse::<usize>().ok().and_then(|i| attributs.get(&i)) {
                        let v = if nom == ".comp_type2" || nom == ".package_type" || nom == ".comp_mount_type" {
                            v.parse::<usize>().ok().and_then(|i| textes.get(&i).cloned()).unwrap_or(v.to_string())
                        } else {
                            v.to_string()
                        };
                        proprietes.push((nom.trim_start_matches('.').replace('_', " "), v));
                    }
                }
            }
            r.push(Composant {
                reference: c.get(5).unwrap_or(&"").to_string(),
                boitier: boitiers.get(pkg).map(|b| b.nom.clone()).unwrap_or_else(|| format!("#{pkg}")),
                piece: c.get(6).unwrap_or(&"").to_string(),
                valeur: String::new(),
                x: f(1) * k,
                y: f(2) * k,
                rotation: f(3),
                dessous: dessous || c.get(4).map(|m| *m == "M").unwrap_or(false),
                broches: 0,
                proprietes,
            });
        } else if let Some(x) = l.strip_prefix("PRP ") {
            if let Some(comp) = r.last_mut() {
                let (nom, valeur) = x.split_once(' ').unwrap_or((x, ""));
                let valeur = valeur.trim().trim_matches('\'').trim().to_string();
                if nom.eq_ignore_ascii_case("VALUE") || nom.eq_ignore_ascii_case("VAL") {
                    comp.valeur = valeur.clone();
                }
                comp.proprietes.push((nom.to_string(), valeur));
            }
        } else if l.starts_with("TOP ") {
            if let Some(comp) = r.last_mut() {
                comp.broches += 1;
            }
        }
    }
    r
}

fn lire_outils(arbre: &Arbre, etape: &str, couche: &str, dessin: &Dessin) -> Vec<OutilOdb> {
    let mut r = Vec::new();
    let Some(t) = arbre.texte(&format!("steps/{etape}/layers/{couche}/tools")) else { return r };
    let mm = t.lines().any(|l| l.trim().eq_ignore_ascii_case("UNITS=MM"));
    let k = if mm { 0.001 } else { 0.0254 };
    // Trous par diamètre, d'après le dessin (pastilles rondes).
    let mut par_diametre: BTreeMap<i64, usize> = BTreeMap::new();
    for e in &dessin.elements {
        if let Prim::Cap { a, b, r } = &e.prim {
            if a.dist(*b) < 1e-9 {
                *par_diametre.entry((r * 2.0 * 1e4).round() as i64).or_default() += 1;
            }
        }
    }
    for (n, c) in blocs(&t) {
        if n != "TOOLS" {
            continue;
        }
        let g = |cle: &str| c.get(cle).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) * k;
        let percage = g("DRILL_SIZE");
        let trous = par_diametre.iter().filter(|(d, _)| ((**d as f64) / 1e4 - percage).abs() < 0.002).map(|(_, n)| *n).sum();
        r.push(OutilOdb {
            numero: c.get("NUM").cloned().unwrap_or_default(),
            genre: match c.get("TYPE").map(|s| s.as_str()) {
                Some("PLATED") => "métallisé".into(),
                Some("NON_PLATED") => "non métallisé".into(),
                Some("VIA") => "via".into(),
                Some(a) => a.to_string(),
                None => String::new(),
            },
            fini: g("FINISH_SIZE"),
            percage,
            trous,
        });
    }
    r
}
