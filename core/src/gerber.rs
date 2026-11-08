//! Lecture des fichiers Gerber : RS-274X, attributs X2 et X3, macros d'ouverture, blocs (AB), répétitions (SR),
//! polarités (LP), transformations (LM, LR, LS), régions (G36/G37), arcs (G74/G75) et commandes anciennes.
//!
//! Référence : The Gerber Layer Format Specification, Ucamco, révision 2026-05.

use crate::geom::*;
use std::collections::{BTreeMap, HashMap};
use std::f64::consts::PI;

/// Résultat de la lecture d'un fichier Gerber.
pub struct Gerber {
    pub dessin: Dessin,
    /// Attributs de fichier (TF), dans l'ordre : nom sans le point initial, valeurs.
    pub attributs: Vec<(String, Vec<String>)>,
    /// Commentaires G04 du début du fichier (avant le premier dessin), utiles comme métadonnées.
    pub commentaires: Vec<String>,
    pub unite: &'static str,
    pub format: String,
    pub ouvertures: Vec<(i32, String, Option<String>)>,
    /// Composants (Gerber X3 : couche composants) : référence -> attributs.
    pub composants: BTreeMap<String, BTreeMap<String, String>>,
    pub nets: Vec<String>,
    pub alertes: Vec<String>,
    pub negatif: bool,
}

#[derive(Clone, Debug)]
enum Ouverture {
    Cercle { d: f64, trou: f64 },
    Rect { w: f64, h: f64, trou: f64 },
    Obrond { w: f64, h: f64, trou: f64 },
    Polygone { d: f64, n: usize, rot: f64, trou: f64 },
    Macro { nom: String, params: Vec<f64> },
    Bloc { dessin: Vec<Element>, infos: Vec<Info> },
}

#[derive(Clone, Debug)]
enum Expr {
    Nombre(f64),
    Var(usize),
    Neg(Box<Expr>),
    Op(char, Box<Expr>, Box<Expr>),
}

impl Expr {
    fn eval(&self, vars: &HashMap<usize, f64>) -> f64 {
        match self {
            Expr::Nombre(v) => *v,
            Expr::Var(i) => *vars.get(i).unwrap_or(&0.0),
            Expr::Neg(e) => -e.eval(vars),
            Expr::Op(o, a, b) => {
                let (a, b) = (a.eval(vars), b.eval(vars));
                match o {
                    '+' => a + b,
                    '-' => a - b,
                    'x' => a * b,
                    '/' => if b != 0.0 { a / b } else { 0.0 },
                    _ => 0.0,
                }
            }
        }
    }
}

/// Analyse d'une expression arithmétique de macro : nombres, $n, + − x / et parenthèses.
struct Analyseur<'a> {
    s: &'a [u8],
    i: usize,
}

impl Analyseur<'_> {
    fn espace(&mut self) {
        while self.i < self.s.len() && (self.s[self.i] as char).is_whitespace() {
            self.i += 1;
        }
    }
    fn somme(&mut self) -> Expr {
        let mut e = self.produit();
        loop {
            self.espace();
            match self.s.get(self.i) {
                Some(b'+') | Some(b'-') => {
                    let o = self.s[self.i] as char;
                    self.i += 1;
                    let d = self.produit();
                    e = Expr::Op(o, Box::new(e), Box::new(d));
                }
                _ => return e,
            }
        }
    }
    fn produit(&mut self) -> Expr {
        let mut e = self.facteur();
        loop {
            self.espace();
            match self.s.get(self.i) {
                Some(b'x') | Some(b'X') | Some(b'/') => {
                    let o = if self.s[self.i] == b'/' { '/' } else { 'x' };
                    self.i += 1;
                    let d = self.facteur();
                    e = Expr::Op(o, Box::new(e), Box::new(d));
                }
                _ => return e,
            }
        }
    }
    fn facteur(&mut self) -> Expr {
        self.espace();
        match self.s.get(self.i) {
            Some(b'(') => {
                self.i += 1;
                let e = self.somme();
                self.espace();
                if self.s.get(self.i) == Some(&b')') {
                    self.i += 1;
                }
                e
            }
            Some(b'-') => {
                self.i += 1;
                Expr::Neg(Box::new(self.facteur()))
            }
            Some(b'+') => {
                self.i += 1;
                self.facteur()
            }
            Some(b'$') => {
                self.i += 1;
                let d = self.i;
                while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
                    self.i += 1;
                }
                Expr::Var(std::str::from_utf8(&self.s[d..self.i]).ok().and_then(|t| t.parse().ok()).unwrap_or(0))
            }
            _ => {
                let d = self.i;
                while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.') {
                    self.i += 1;
                }
                Expr::Nombre(std::str::from_utf8(&self.s[d..self.i]).ok().and_then(|t| t.parse().ok()).unwrap_or(0.0))
            }
        }
    }
}

fn expr(t: &str) -> Expr {
    Analyseur { s: t.as_bytes(), i: 0 }.somme()
}

#[derive(Clone, Debug)]
enum Instruction {
    Primitive(u32, Vec<Expr>),
    Affectation(usize, Expr),
}

#[derive(Clone, Copy, PartialEq)]
enum Interpolation {
    Lineaire,
    Horaire,
    Antihoraire,
}

/// Contexte d'enregistrement : le fichier lui-même, un bloc d'ouverture (AB) ou une répétition (SR).
struct Cadre {
    elements: Vec<Element>,
    infos: Vec<Info>,
    genre: Genre,
}

enum Genre {
    Racine,
    Bloc(i32),
    Repetition { nx: usize, ny: usize, dx: f64, dy: f64 },
}

struct Lecteur {
    // Format des coordonnées
    entiers: usize,
    decimales: usize,
    zeros_tete_omis: bool,
    incremental: bool,
    facteur: f64,
    unite_definie: bool,
    // État graphique
    ouvertures: HashMap<i32, Ouverture>,
    macros: HashMap<String, Vec<Instruction>>,
    courante: Option<i32>,
    interpolation: Interpolation,
    multi_quadrant: bool,
    region: bool,
    contour: Vec<Pt>,
    contours: Vec<Vec<Pt>>,
    x: f64,
    y: f64,
    sombre: bool,
    miroir_x: bool,
    miroir_y: bool,
    rotation: f64,
    echelle: f64,
    // Attributs
    attr_ouverture: BTreeMap<String, String>,
    attr_objet: BTreeMap<String, String>,
    attr_ouvertures: HashMap<i32, BTreeMap<String, String>>,
    formes: HashMap<i32, Vec<Prim>>,
    // Sortie
    cadres: Vec<Cadre>,
    objets_info: Vec<Info>,
    infos_index: HashMap<Info, u32>,
    res: Gerber,
    dessin_commence: bool,
    alerte_d: bool,
}

/// Lit un fichier Gerber.
pub fn lire(texte: &str) -> Gerber {
    let mut l = Lecteur {
        entiers: 3,
        decimales: 6,
        zeros_tete_omis: true,
        incremental: false,
        facteur: 1.0,
        unite_definie: false,
        ouvertures: HashMap::new(),
        macros: HashMap::new(),
        courante: None,
        interpolation: Interpolation::Lineaire,
        multi_quadrant: false,
        region: false,
        contour: Vec::new(),
        contours: Vec::new(),
        x: 0.0,
        y: 0.0,
        sombre: true,
        miroir_x: false,
        miroir_y: false,
        rotation: 0.0,
        echelle: 1.0,
        attr_ouverture: BTreeMap::new(),
        attr_objet: BTreeMap::new(),
        attr_ouvertures: HashMap::new(),
        formes: HashMap::new(),
        cadres: vec![Cadre { elements: Vec::new(), infos: Vec::new(), genre: Genre::Racine }],
        objets_info: Vec::new(),
        infos_index: HashMap::new(),
        res: Gerber {
            dessin: Dessin::default(),
            attributs: Vec::new(),
            commentaires: Vec::new(),
            unite: "mm",
            format: String::new(),
            ouvertures: Vec::new(),
            composants: BTreeMap::new(),
            nets: Vec::new(),
            alertes: Vec::new(),
            negatif: false,
        },
        dessin_commence: false,
        alerte_d: false,
    };
    l.lire(texte);
    l.fin()
}

impl Lecteur {
    fn alerte(&mut self, m: impl Into<String>) {
        let m = m.into();
        if self.res.alertes.len() < 50 && !self.res.alertes.contains(&m) {
            self.res.alertes.push(m);
        }
    }

    fn lire(&mut self, texte: &str) {
        let b = texte.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            if c == b'%' {
                // Commande étendue : %...*...*%, éventuellement plusieurs blocs.
                let fin = match b[i + 1..].iter().position(|&x| x == b'%') {
                    Some(p) => i + 1 + p,
                    None => b.len(),
                };
                let contenu = &texte[i + 1..fin];
                self.etendue(contenu);
                i = fin + 1;
            } else if c.is_ascii_whitespace() {
                i += 1;
            } else {
                let fin = match b[i..].iter().position(|&x| x == b'*') {
                    Some(p) => i + p,
                    None => b.len(),
                };
                let brut = texte[i..fin].trim();
                if let Some(t) = brut.strip_prefix("G04").or_else(|| brut.strip_prefix("G4 ")) {
                    if !self.dessin_commence && self.res.commentaires.len() < 30 && !t.trim().is_empty() {
                        self.res.commentaires.push(t.trim().to_string());
                    }
                    i = fin + 1;
                    continue;
                }
                let mot: String = brut.chars().filter(|c| !c.is_whitespace()).collect();
                if !mot.is_empty() {
                    if !self.mot(&mot) {
                        return;
                    }
                }
                i = fin + 1;
            }
        }
    }

    fn etendue(&mut self, contenu: &str) {
        let nom = contenu.trim_start().get(0..2).unwrap_or("").to_string();
        if nom == "AM" {
            self.definir_macro(contenu.trim_start());
            return;
        }
        for bloc in contenu.split('*') {
            let bloc = bloc.trim();
            if bloc.is_empty() {
                continue;
            }
            self.commande_etendue(bloc);
        }
    }

    fn commande_etendue(&mut self, c: &str) {
        let code = c.get(0..2).unwrap_or("");
        let reste = &c[code.len().min(c.len())..];
        match code {
            "FS" => self.format(reste),
            "MO" => {
                self.facteur = if reste.starts_with("IN") { 25.4 } else { 1.0 };
                self.res.unite = if reste.starts_with("IN") { "pouce" } else { "mm" };
                self.unite_definie = true;
            }
            "AD" => self.definir_ouverture(reste),
            "AB" => {
                if reste.is_empty() {
                    self.fin_cadre();
                } else {
                    let n = reste.trim_start_matches('D').parse().unwrap_or(0);
                    self.cadres.push(Cadre { elements: Vec::new(), infos: Vec::new(), genre: Genre::Bloc(n) });
                }
            }
            "SR" => {
                // Fin de la répétition en cours, s'il y en a une, puis début de la suivante.
                if matches!(self.cadres.last().map(|c| &c.genre), Some(Genre::Repetition { .. })) {
                    self.fin_cadre();
                }
                if !reste.is_empty() {
                    let mut nx = 1;
                    let mut ny = 1;
                    let mut dx = 0.0;
                    let mut dy = 0.0;
                    for (lettre, valeur) in champs(reste) {
                        match lettre {
                            'X' => nx = valeur.parse::<f64>().unwrap_or(1.0) as usize,
                            'Y' => ny = valeur.parse::<f64>().unwrap_or(1.0) as usize,
                            'I' => dx = valeur.parse::<f64>().unwrap_or(0.0) * self.facteur,
                            'J' => dy = valeur.parse::<f64>().unwrap_or(0.0) * self.facteur,
                            _ => {}
                        }
                    }
                    if nx > 1 || ny > 1 {
                        self.cadres.push(Cadre {
                            elements: Vec::new(),
                            infos: Vec::new(),
                            genre: Genre::Repetition { nx: nx.max(1), ny: ny.max(1), dx, dy },
                        });
                    }
                }
            }
            "LP" => self.sombre = !reste.starts_with('C'),
            "LM" => {
                self.miroir_x = reste.contains('X');
                self.miroir_y = reste.contains('Y');
            }
            "LR" => self.rotation = reste.parse::<f64>().unwrap_or(0.0).to_radians(),
            "LS" => self.echelle = reste.parse::<f64>().unwrap_or(1.0),
            "TF" => {
                let (nom, valeurs) = attribut(reste);
                if nom == ".Part" || nom == ".FileFunction" || !nom.is_empty() {
                    self.res.attributs.retain(|(n, _)| *n != nom.trim_start_matches('.'));
                    self.res.attributs.push((nom.trim_start_matches('.').to_string(), valeurs));
                }
            }
            "TA" => {
                let (nom, valeurs) = attribut(reste);
                self.attr_ouverture.insert(nom, valeurs.join(","));
            }
            "TO" => {
                let (nom, valeurs) = attribut(reste);
                self.attr_objet.insert(nom, valeurs.join(","));
            }
            "TD" => {
                let nom = reste.trim();
                if nom.is_empty() {
                    self.attr_ouverture.clear();
                    self.attr_objet.clear();
                } else {
                    self.attr_ouverture.remove(nom);
                    self.attr_objet.remove(nom);
                }
            }
            "IP" => {
                if reste.starts_with("NEG") {
                    self.res.negatif = true;
                    self.alerte("Image négative (IP NEG, commande ancienne) : le fichier décrit les zones sans cuivre.");
                }
            }
            "IN" | "LN" | "IR" | "AS" | "MI" | "SF" | "OF" | "IF" => {
                if c.len() > 2 && (code == "IR" && reste != "0" || code == "OF" && !reste.chars().all(|c| "AB0.".contains(c)) || code == "MI" && reste.contains('1') || code == "SF" && reste != "A1B1") {
                    self.alerte(format!("Commande ancienne %{code}% ignorée : le dessin peut être décalé ou tourné."));
                }
            }
            "G0" | "G1" | "G3" | "G7" | "D0" | "D1" | "M0" | "X0" => {
                // Mot ordinaire entouré de % (vu dans certains fichiers anciens).
                self.mot(c);
            }
            _ => {
                if c.starts_with('G') || c.starts_with('D') || c.starts_with('X') || c.starts_with('Y') {
                    self.mot(c);
                }
            }
        }
    }

    fn format(&mut self, r: &str) {
        self.zeros_tete_omis = !r.starts_with('T');
        self.incremental = r.get(1..2) == Some("I");
        if let Some(p) = r.find('X') {
            let ch: Vec<u32> = r[p + 1..].chars().take(2).filter_map(|c| c.to_digit(10)).collect();
            if ch.len() == 2 {
                self.entiers = ch[0] as usize;
                self.decimales = ch[1] as usize;
            }
        }
        self.res.format = format!("{}.{}{}", self.entiers, self.decimales, if self.zeros_tete_omis { "" } else { " (zéros de fin omis)" });
        if self.incremental {
            self.alerte("Coordonnées incrémentales (FSI, ancienne notation).");
        }
    }

    fn coord(&self, t: &str) -> f64 {
        let v = if t.contains('.') {
            t.parse::<f64>().unwrap_or(0.0)
        } else {
            let (signe, chiffres) = match t.as_bytes().first() {
                Some(b'-') => (-1.0, &t[1..]),
                Some(b'+') => (1.0, &t[1..]),
                _ => (1.0, t),
            };
            let n = if self.zeros_tete_omis {
                chiffres.parse::<f64>().unwrap_or(0.0)
            } else {
                let total = self.entiers + self.decimales;
                let mut s = chiffres.to_string();
                while s.len() < total {
                    s.push('0');
                }
                s.parse::<f64>().unwrap_or(0.0)
            };
            signe * n / 10f64.powi(self.decimales as i32)
        };
        v * self.facteur
    }

    fn definir_ouverture(&mut self, r: &str) {
        // ADD10C,0.5X0.2
        let r = r.trim_start_matches('D');
        let fin_num = r.find(|c: char| !c.is_ascii_digit()).unwrap_or(r.len());
        let num: i32 = r[..fin_num].parse().unwrap_or(0);
        let reste = &r[fin_num..];
        let (forme, params) = match reste.find(',') {
            Some(p) => (&reste[..p], &reste[p + 1..]),
            None => (reste, ""),
        };
        let p: Vec<f64> = params.split('X').filter(|s| !s.is_empty()).map(|s| s.trim().parse::<f64>().unwrap_or(0.0)).collect();
        let k = self.facteur;
        let g = |i: usize| p.get(i).copied().unwrap_or(0.0);
        let ouv = match forme {
            "C" => Ouverture::Cercle { d: g(0) * k, trou: g(1) * k },
            "R" => Ouverture::Rect { w: g(0) * k, h: g(1) * k, trou: g(2) * k },
            "O" => Ouverture::Obrond { w: g(0) * k, h: g(1) * k, trou: g(2) * k },
            "P" => Ouverture::Polygone { d: g(0) * k, n: g(1) as usize, rot: g(2).to_radians(), trou: g(3) * k },
            nom => {
                if !self.macros.contains_key(nom) {
                    self.alerte(format!("Macro d'ouverture « {nom} » inconnue (D{num})."));
                }
                Ouverture::Macro { nom: nom.to_string(), params: p.clone() }
            }
        };
        let description = decrire(&ouv, self.res.unite);
        let fonction = self.attr_ouverture.get(".AperFunction").cloned();
        self.res.ouvertures.retain(|(n, _, _)| *n != num);
        self.res.ouvertures.push((num, description, fonction));
        self.attr_ouvertures.insert(num, self.attr_ouverture.clone());
        self.formes.remove(&num);
        self.ouvertures.insert(num, ouv);
    }

    fn definir_macro(&mut self, c: &str) {
        let mut blocs = c.split('*');
        let nom = blocs.next().unwrap_or("").trim()[2..].trim().to_string();
        let mut instructions = Vec::new();
        for bloc in blocs {
            let bloc: String = bloc.chars().filter(|c| !c.is_whitespace()).collect();
            if bloc.is_empty() || bloc.starts_with('0') && !bloc.starts_with("0.") && bloc.chars().nth(1).map(|c| c == ' ' || !c.is_ascii_digit()).unwrap_or(true) && !bloc.contains(',') {
                continue;
            }
            if bloc.starts_with('$') {
                if let Some(p) = bloc.find('=') {
                    let n = bloc[1..p].parse().unwrap_or(0);
                    instructions.push(Instruction::Affectation(n, expr(&bloc[p + 1..])));
                }
                continue;
            }
            let mut champs = bloc.split(',');
            let code: u32 = champs.next().unwrap_or("").parse().unwrap_or(999);
            if code == 0 {
                continue; // commentaire
            }
            instructions.push(Instruction::Primitive(code, champs.map(expr).collect()));
        }
        self.macros.insert(nom, instructions);
    }

    /// Traite un mot (bloc terminé par *). Renvoie false à la fin du fichier (M02).
    fn mot(&mut self, m: &str) -> bool {
        if m.starts_with("G04") || m.starts_with("G4 ") {
            if !self.dessin_commence && self.res.commentaires.len() < 30 {
                let t = m[3..].trim().to_string();
                if !t.is_empty() {
                    self.res.commentaires.push(t);
                }
            }
            return true;
        }
        if m == "M02" || m == "M2" || m == "M00" || m == "M30" {
            return false;
        }
        if m == "M01" || m == "M1" {
            return true;
        }
        let mut x: Option<f64> = None;
        let mut y: Option<f64> = None;
        let mut i_: Option<f64> = None;
        let mut j_: Option<f64> = None;
        let mut d: Option<i32> = None;
        for (lettre, valeur) in champs(m) {
            match lettre {
                'G' => {
                    let g: i32 = valeur.parse().unwrap_or(-1);
                    match g {
                        1 => self.interpolation = Interpolation::Lineaire,
                        2 => self.interpolation = Interpolation::Horaire,
                        3 => self.interpolation = Interpolation::Antihoraire,
                        74 => self.multi_quadrant = false,
                        75 => self.multi_quadrant = true,
                        36 => {
                            self.region = true;
                            self.contour.clear();
                            self.contours.clear();
                        }
                        37 => {
                            self.fermer_contour();
                            self.emettre_region();
                            self.region = false;
                        }
                        70 => {
                            self.facteur = 25.4;
                            self.res.unite = "pouce";
                        }
                        71 => {
                            self.facteur = 1.0;
                            self.res.unite = "mm";
                        }
                        90 => self.incremental = false,
                        91 => self.incremental = true,
                        54 | 55 | 4 => {}
                        _ => {}
                    }
                }
                'X' => x = Some(self.coord(valeur)),
                'Y' => y = Some(self.coord(valeur)),
                'I' => i_ = Some(self.coord(valeur)),
                'J' => j_ = Some(self.coord(valeur)),
                'D' => d = valeur.parse().ok(),
                'M' => {
                    if valeur == "02" || valeur == "2" || valeur == "00" {
                        return false;
                    }
                }
                _ => {}
            }
        }
        if let Some(n) = d {
            if n >= 10 {
                self.courante = Some(n);
                return true;
            }
        }
        let nx = match x {
            Some(v) if self.incremental => self.x + v,
            Some(v) => v,
            None => self.x,
        };
        let ny = match y {
            Some(v) if self.incremental => self.y + v,
            Some(v) => v,
            None => self.y,
        };
        // Coordonnées sans code D : D01 implicite (usage ancien).
        let operation = d.or(if x.is_some() || y.is_some() { Some(1) } else { None });
        match operation {
            Some(1) => {
                self.dessin_commence = true;
                self.interpoler(pt(nx, ny), i_.unwrap_or(0.0), j_.unwrap_or(0.0));
            }
            Some(2) => {
                if self.region {
                    self.fermer_contour();
                }
                self.x = nx;
                self.y = ny;
            }
            Some(3) => {
                self.dessin_commence = true;
                self.x = nx;
                self.y = ny;
                self.flasher(pt(nx, ny));
            }
            _ => {}
        }
        true
    }

    fn info_objet(&mut self, genre: &str) -> Info {
        let mut info: Info = vec![("Objet".into(), genre.into())];
        if let Some(n) = self.courante {
            if !self.region {
                if let Some(o) = self.ouvertures.get(&n) {
                    info.push(("Ouverture".into(), format!("D{n} : {}", decrire(o, self.res.unite))));
                }
            }
            let attrs = if self.region { None } else { self.attr_ouvertures.get(&n).cloned() };
            if let Some(a) = attrs {
                for (k, v) in a {
                    info.push((libelle_attribut(&k), v));
                }
            }
        }
        if self.region {
            for (k, v) in self.attr_ouverture.clone() {
                info.push((libelle_attribut(&k), v));
            }
        }
        for (k, v) in &self.attr_objet {
            info.push((libelle_attribut(k), v.clone()));
        }
        if !self.sombre {
            info.push(("Polarité".into(), "claire (efface)".into()));
        }
        info
    }

    fn nouvel_objet(&mut self, genre: &str) -> u32 {
        let info = self.info_objet(genre);
        if let Some(net) = self.attr_objet.get(".N") {
            if !net.is_empty() && self.res.nets.len() < 100_000 && !self.res.nets.contains(net) {
                self.res.nets.push(net.clone());
            }
        }
        let id = match self.infos_index.get(&info) {
            Some(&i) => i,
            None => {
                let i = self.objets_info.len() as u32;
                self.infos_index.insert(info.clone(), i);
                self.objets_info.push(info);
                i
            }
        };
        id
    }

    fn emettre(&mut self, prim: Prim, sombre: bool, info: u32) {
        let cadre = self.cadres.last_mut().unwrap();
        cadre.elements.push(Element { prim, sombre, objet: info });
    }

    fn interpoler(&mut self, fin: Pt, i: f64, j: f64) {
        let debut = pt(self.x, self.y);
        self.x = fin.x;
        self.y = fin.y;
        let points = match self.interpolation {
            Interpolation::Lineaire => vec![debut, fin],
            sens => {
                let horaire = sens == Interpolation::Horaire;
                let centre = if self.multi_quadrant {
                    pt(debut.x + i, debut.y + j)
                } else {
                    centre_quadrant_unique(debut, fin, i.abs(), j.abs(), horaire)
                };
                if self.multi_quadrant && debut.dist(fin) < 1e-9 {
                    arc(debut, fin, centre, horaire)
                } else if !self.multi_quadrant && debut.dist(fin) < 1e-9 {
                    vec![debut, fin]
                } else {
                    arc(debut, fin, centre, horaire)
                }
            }
        };
        if self.region {
            if self.contour.is_empty() {
                self.contour.push(debut);
            }
            self.contour.extend_from_slice(&points[1..]);
            return;
        }
        let Some(n) = self.courante else {
            if !self.alerte_d {
                self.alerte_d = true;
                self.alerte("Tracé sans ouverture sélectionnée : ignoré.");
            }
            return;
        };
        let ouv = self.ouvertures.get(&n).cloned();
        let genre = if self.interpolation == Interpolation::Lineaire { "Trait" } else { "Arc" };
        let info = self.nouvel_objet(genre);
        let sombre = self.sombre;
        match ouv {
            Some(Ouverture::Cercle { d, .. }) => {
                let r = d * self.echelle / 2.0;
                for w in points.windows(2) {
                    self.emettre(Prim::Cap { a: w[0], b: w[1], r }, sombre, info);
                }
            }
            Some(Ouverture::Rect { w, h, .. }) => {
                // Rectangle balayé (usage déconseillé mais valide) : enveloppe convexe des deux rectangles.
                let (hw, hh) = (w * self.echelle / 2.0, h * self.echelle / 2.0);
                for seg in points.windows(2) {
                    let mut coins = Vec::new();
                    for p in seg {
                        for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                            coins.push(pt(p.x + sx * hw, p.y + sy * hh));
                        }
                    }
                    self.emettre(Prim::Poly { anneaux: vec![enveloppe_convexe(coins)] }, sombre, info);
                }
            }
            Some(_) => {
                self.alerte(format!("Tracé avec l'ouverture D{n}, qui n'est ni un cercle ni un rectangle : dessiné en trait fin."));
                for w in points.windows(2) {
                    self.emettre(Prim::Cap { a: w[0], b: w[1], r: 0.0 }, sombre, info);
                }
            }
            None => {
                self.alerte(format!("Ouverture D{n} non définie : tracés dessinés en trait fin (fichier RS-274D sans ses ouvertures ?)."));
                for w in points.windows(2) {
                    self.emettre(Prim::Cap { a: w[0], b: w[1], r: 0.0 }, sombre, info);
                }
            }
        }
    }

    fn fermer_contour(&mut self) {
        if self.contour.len() >= 3 {
            let c = std::mem::take(&mut self.contour);
            self.contours.push(c);
        }
        self.contour.clear();
    }

    fn emettre_region(&mut self) {
        if self.contours.is_empty() {
            return;
        }
        let info = self.nouvel_objet("Région");
        let sombre = self.sombre;
        for c in std::mem::take(&mut self.contours) {
            self.emettre(Prim::Poly { anneaux: vec![c] }, sombre, info);
        }
    }

    /// Transformation d'objet courante (LM, puis LR, puis LS), placée en `p`.
    fn transfo_objet(&self, p: Pt) -> Transfo {
        Transfo::translation(p.x, p.y)
            .apres(&Transfo::echelle(self.echelle))
            .apres(&Transfo::rotation(self.rotation))
            .apres(&Transfo::miroir(self.miroir_x, self.miroir_y))
    }

    fn flasher(&mut self, p: Pt) {
        let Some(n) = self.courante else {
            self.alerte("Flash sans ouverture sélectionnée : ignoré.");
            return;
        };
        let Some(ouv) = self.ouvertures.get(&n).cloned() else {
            self.alerte(format!("Ouverture D{n} non définie : flashs dessinés en point (fichier RS-274D sans ses ouvertures ?)."));
            let info = self.nouvel_objet("Flash");
            self.emettre(Prim::Cap { a: p, b: p, r: 0.0 }, self.sombre, info);
            return;
        };
        let t = self.transfo_objet(p);
        let sombre = self.sombre;
        if let Ouverture::Bloc { dessin, infos } = &ouv {
            // Les objets du bloc gardent leurs propres attributs ; polarités inversées si le flash est clair.
            let base = self.objets_info.len() as u32;
            for info in infos {
                self.objets_info.push(info.clone());
            }
            for e in dessin.clone() {
                let s = if sombre { e.sombre } else { !e.sombre };
                self.emettre(e.prim.transformee(&t), s, base + e.objet);
            }
            let _ = infos;
            return;
        }
        let info = self.nouvel_objet("Flash");
        // Les parties non exposées d'une ouverture (trou, primitive d'exposition 0) sont des trous transparents : la
        // forme exacte est calculée une fois par ouverture, puis dessinée entièrement dans la polarité courante.
        let forme = match self.formes.get(&n) {
            Some(f) => f.clone(),
            None => {
                let brute = forme_ouverture(&ouv, &self.macros);
                let f: Vec<Prim> = if brute.iter().any(|(_, e)| !e) { forme_booleenne(&brute) } else { brute.into_iter().map(|(p, _)| p).collect() };
                self.formes.insert(n, f.clone());
                f
            }
        };
        for prim in forme {
            self.emettre(prim.transformee(&t), sombre, info);
        }
    }

    fn fin_cadre(&mut self) {
        if self.cadres.len() <= 1 {
            return;
        }
        let cadre = self.cadres.pop().unwrap();
        match cadre.genre {
            Genre::Bloc(n) => {
                // Les numéros d'objet du bloc sont des indices dans objets_info : on les rend locaux au bloc.
                let mut locaux: HashMap<u32, u32> = HashMap::new();
                let mut infos = Vec::new();
                let mut elements = Vec::new();
                for e in cadre.elements {
                    let id = *locaux.entry(e.objet).or_insert_with(|| {
                        infos.push(self.objets_info[e.objet as usize].clone());
                        (infos.len() - 1) as u32
                    });
                    elements.push(Element { objet: id, ..e });
                }
                self.ouvertures.insert(n, Ouverture::Bloc { dessin: elements, infos });
                self.res.ouvertures.retain(|(k, _, _)| *k != n);
                self.res.ouvertures.push((n, "bloc d'ouverture (AB)".into(), None));
            }
            Genre::Repetition { nx, ny, dx, dy } => {
                for j in 0..ny {
                    for i in 0..nx {
                        let t = Transfo::translation(i as f64 * dx, j as f64 * dy);
                        for e in &cadre.elements {
                            let prim = e.prim.transformee(&t);
                            self.emettre(prim, e.sombre, e.objet);
                        }
                    }
                }
            }
            Genre::Racine => {}
        }
    }

    fn fin(mut self) -> Gerber {
        if self.region {
            self.fermer_contour();
            self.emettre_region();
        }
        while self.cadres.len() > 1 {
            self.fin_cadre();
        }
        if !self.unite_definie && self.res.format.is_empty() {
            self.alerte("Ni format (FS) ni unité (MO) : fichier RS-274D ou incomplet, dessin peut-être faux.");
        }
        let racine = self.cadres.pop().unwrap();
        let mut dessin = Dessin::default();
        let mut correspondance: HashMap<u32, u32> = HashMap::new();
        for e in racine.elements {
            let id = match correspondance.get(&e.objet) {
                Some(&i) => i,
                None => {
                    let i = dessin.nouvel_objet(self.objets_info[e.objet as usize].clone());
                    correspondance.insert(e.objet, i);
                    i
                }
            };
            dessin.ajoute(e.prim, e.sombre, id);
        }
        // Composants (X3) : attributs .C, .CRot, .CMfr… relevés sur les objets.
        for info in &dessin.infos {
            let mut refdes = None;
            let mut attrs = BTreeMap::new();
            for (k, v) in info {
                if k == "Composant" {
                    refdes = Some(v.clone());
                } else if k.starts_with("Composant ·") {
                    attrs.insert(k.trim_start_matches("Composant · ").to_string(), v.clone());
                }
            }
            if let Some(r) = refdes {
                let e = self.res.composants.entry(r).or_default();
                for (k, v) in attrs {
                    e.insert(k, v);
                }
            }
        }
        self.res.dessin = dessin;
        self.res.ouvertures.sort_by_key(|o| o.0);
        self.res
    }
}

/// Lettres et valeurs d'un mot : « G01X100Y-20D01 » -> [('G',"01"), ('X',"100"), ('Y',"-20"), ('D',"01")].
fn champs(m: &str) -> Vec<(char, &str)> {
    let mut r = Vec::new();
    let b = m.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_alphabetic() {
            let d = i + 1;
            let mut f = d;
            while f < b.len() && (b[f].is_ascii_digit() || b[f] == b'.' || b[f] == b'-' || b[f] == b'+') {
                f += 1;
            }
            r.push((c.to_ascii_uppercase(), &m[d..f]));
            i = f.max(i + 1);
        } else {
            i += 1;
        }
    }
    r
}

fn attribut(r: &str) -> (String, Vec<String>) {
    let mut parties = r.split(',');
    let nom = parties.next().unwrap_or("").trim().to_string();
    (nom, parties.map(|s| s.trim().to_string()).collect())
}

/// Libellé français d'un attribut X2/X3.
pub fn libelle_attribut(k: &str) -> String {
    match k {
        ".N" => "Net".into(),
        ".P" => "Broche".into(),
        ".C" => "Composant".into(),
        ".AperFunction" => "Fonction".into(),
        ".DrillTolerance" => "Tolérance de perçage".into(),
        ".FlashText" => "Texte".into(),
        ".CRot" => "Composant · rotation".into(),
        ".CMfr" => "Composant · fabricant".into(),
        ".CMPN" => "Composant · référence fabricant".into(),
        ".CVal" => "Composant · valeur".into(),
        ".CMnt" => "Composant · montage".into(),
        ".CFtp" => "Composant · empreinte".into(),
        ".CPgN" => "Composant · boîtier".into(),
        ".CPgD" => "Composant · description du boîtier".into(),
        ".CHgt" => "Composant · hauteur".into(),
        ".CLbN" => "Composant · bibliothèque".into(),
        ".CLbD" => "Composant · description".into(),
        ".CSup" => "Composant · fournisseur".into(),
        ".PinFunction" | ".CPinFunc" => "Broche · fonction".into(),
        _ => k.trim_start_matches('.').to_string(),
    }
}

fn decrire(o: &Ouverture, unite: &str) -> String {
    let v = |x: f64| -> String {
        if unite == "pouce" {
            format!("{:.4} mm ({:.2} mil)", x, x / 0.0254)
        } else {
            format!("{} mm", nombre(x))
        }
    };
    match o {
        Ouverture::Cercle { d, trou } => format!("cercle Ø{}{}", v(*d), if *trou > 0.0 { format!(", trou Ø{}", v(*trou)) } else { String::new() }),
        Ouverture::Rect { w, h, .. } => format!("rectangle {} × {}", v(*w), v(*h)),
        Ouverture::Obrond { w, h, .. } => format!("oblong {} × {}", v(*w), v(*h)),
        Ouverture::Polygone { d, n, .. } => format!("polygone à {n} côtés, Ø{}", v(*d)),
        Ouverture::Macro { nom, params } => {
            if params.is_empty() { format!("macro {nom}") } else { format!("macro {nom} ({})", params.iter().map(|p| nombre(*p)).collect::<Vec<_>>().join(" × ")) }
        }
        Ouverture::Bloc { .. } => "bloc d'ouverture".into(),
    }
}

pub fn nombre(x: f64) -> String {
    let s = format!("{:.4}", x);
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    s.replace('.', ",")
}

/// Forme d'une ouverture standard ou de macro, centrée sur l'origine : (primitive, exposée ?).
fn forme_ouverture(o: &Ouverture, macros: &HashMap<String, Vec<Instruction>>) -> Vec<(Prim, bool)> {
    let o0 = pt(0.0, 0.0);
    let mut r = Vec::new();
    let trou = |t: f64, r: &mut Vec<(Prim, bool)>| {
        if t > 0.0 {
            r.push((Prim::Cap { a: o0, b: o0, r: t / 2.0 }, false));
        }
    };
    match o {
        Ouverture::Cercle { d, trou: t } => {
            r.push((Prim::Cap { a: o0, b: o0, r: d / 2.0 }, true));
            trou(*t, &mut r);
        }
        Ouverture::Rect { w, h, trou: t } => {
            r.push((Prim::Boite { c: o0, hw: w / 2.0, hh: h / 2.0, ang: 0.0, rad: 0.0 }, true));
            trou(*t, &mut r);
        }
        Ouverture::Obrond { w, h, trou: t } => {
            r.push((Prim::Boite { c: o0, hw: w / 2.0, hh: h / 2.0, ang: 0.0, rad: w.min(*h) / 2.0 }, true));
            trou(*t, &mut r);
        }
        Ouverture::Polygone { d, n, rot, trou: t } => {
            r.push((Prim::Poly { anneaux: vec![polygone_regulier(o0, *d, *n, *rot)] }, true));
            trou(*t, &mut r);
        }
        Ouverture::Macro { nom, params } => {
            if let Some(instr) = macros.get(nom) {
                r = executer_macro(instr, params);
            }
        }
        Ouverture::Bloc { .. } => {}
    }
    r
}

fn executer_macro(instructions: &[Instruction], params: &[f64]) -> Vec<(Prim, bool)> {
    let mut vars: HashMap<usize, f64> = params.iter().enumerate().map(|(i, v)| (i + 1, *v)).collect();
    let mut r = Vec::new();
    let o0 = pt(0.0, 0.0);
    for ins in instructions {
        match ins {
            Instruction::Affectation(n, e) => {
                let v = e.eval(&vars);
                vars.insert(*n, v);
            }
            Instruction::Primitive(code, args) => {
                let a: Vec<f64> = args.iter().map(|e| e.eval(&vars)).collect();
                let g = |i: usize| a.get(i).copied().unwrap_or(0.0);
                let expose = |v: f64| v != 0.0;
                let rot = |angle: f64| Transfo::rotation(angle.to_radians());
                match code {
                    1 => {
                        // Cercle : exposition, diamètre, centre x, y, [rotation]
                        let t = rot(g(4));
                        r.push((Prim::Cap { a: t.applique(pt(g(2), g(3))), b: t.applique(pt(g(2), g(3))), r: g(1) / 2.0 }, expose(g(0))));
                    }
                    2 | 20 => {
                        // Ligne vectorielle : exposition, largeur, début, fin, rotation
                        let (s, e) = (pt(g(2), g(3)), pt(g(4), g(5)));
                        let ang = (e.y - s.y).atan2(e.x - s.x);
                        let prim = Prim::Boite { c: pt((s.x + e.x) / 2.0, (s.y + e.y) / 2.0), hw: s.dist(e) / 2.0, hh: g(1) / 2.0, ang, rad: 0.0 };
                        r.push((prim.transformee(&rot(g(6))), expose(g(0))));
                    }
                    21 => {
                        // Ligne centrée : exposition, largeur, hauteur, centre x, y, rotation
                        let prim = Prim::Boite { c: pt(g(3), g(4)), hw: g(1) / 2.0, hh: g(2) / 2.0, ang: 0.0, rad: 0.0 };
                        r.push((prim.transformee(&rot(g(5))), expose(g(0))));
                    }
                    22 => {
                        // Ligne coin inférieur gauche (ancienne) : exposition, largeur, hauteur, x, y, rotation
                        let prim = Prim::Boite { c: pt(g(3) + g(1) / 2.0, g(4) + g(2) / 2.0), hw: g(1) / 2.0, hh: g(2) / 2.0, ang: 0.0, rad: 0.0 };
                        r.push((prim.transformee(&rot(g(5))), expose(g(0))));
                    }
                    4 => {
                        // Contour : exposition, n, (n+1) points, rotation
                        let n = g(1) as usize;
                        let pts: Vec<Pt> = (0..=n).map(|i| pt(g(2 + 2 * i), g(3 + 2 * i))).collect();
                        let prim = Prim::Poly { anneaux: vec![pts] };
                        r.push((prim.transformee(&rot(g(4 + 2 * n))), expose(g(0))));
                    }
                    5 => {
                        // Polygone : exposition, n sommets, centre x, y, diamètre, rotation
                        let prim = Prim::Poly { anneaux: vec![polygone_regulier(pt(g(2), g(3)), g(4), g(1) as usize, 0.0)] };
                        r.push((prim.transformee(&rot(g(5))), expose(g(0))));
                    }
                    6 => {
                        // Moiré (ancien) : centre, Ø extérieur, épaisseur, écart, nb d'anneaux, croix (épaisseur, longueur), rotation
                        let c = pt(g(0), g(1));
                        let t = rot(g(8));
                        let mut d = g(2);
                        for _ in 0..(g(5) as usize).max(1) {
                            if d <= 0.0 {
                                break;
                            }
                            let int = d - 2.0 * g(3);
                            let mut anneaux = vec![cercle(c, d / 2.0)];
                            if int > 0.0 {
                                anneaux.push(cercle(c, int / 2.0));
                            }
                            r.push((Prim::Poly { anneaux }.transformee(&t), true));
                            d = int - 2.0 * g(4);
                        }
                        r.push((Prim::Boite { c, hw: g(7) / 2.0, hh: g(6) / 2.0, ang: 0.0, rad: 0.0 }.transformee(&t), true));
                        r.push((Prim::Boite { c, hw: g(6) / 2.0, hh: g(7) / 2.0, ang: 0.0, rad: 0.0 }.transformee(&t), true));
                    }
                    7 => {
                        // Thermique : centre, Ø extérieur, Ø intérieur, écart, rotation
                        let c = pt(g(0), g(1));
                        let ext = cercle(c, g(2) / 2.0);
                        let int = cercle(c, g(3) / 2.0);
                        for p in thermique(c, &ext, &int, 4, 0.0, g(4)) {
                            r.push((p.transformee(&rot(g(5))), true));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let _ = o0;
    r
}

/// Centre d'un arc en mode quadrant unique (G74) : parmi les quatre signes de (i, j), celui qui donne un arc d'au
/// plus 90° dans le bon sens, avec des rayons de début et de fin les plus proches.
fn centre_quadrant_unique(debut: Pt, fin: Pt, i: f64, j: f64, horaire: bool) -> Pt {
    let mut meilleur = pt(debut.x + i, debut.y + j);
    let mut ecart = f64::INFINITY;
    for (si, sj) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        let c = pt(debut.x + si * i, debut.y + sj * j);
        let a0 = (debut.y - c.y).atan2(debut.x - c.x);
        let a1 = (fin.y - c.y).atan2(fin.x - c.x);
        let mut b = if horaire { a0 - a1 } else { a1 - a0 };
        while b < 0.0 {
            b += 2.0 * PI;
        }
        if b <= PI / 2.0 + 1e-6 {
            let e = (debut.dist(c) - fin.dist(c)).abs();
            if e < ecart {
                ecart = e;
                meilleur = c;
            }
        }
    }
    meilleur
}

pub fn enveloppe_convexe(mut p: Vec<Pt>) -> Vec<Pt> {
    p.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap().then(a.y.partial_cmp(&b.y).unwrap()));
    p.dedup_by(|a, b| a.dist(*b) < 1e-12);
    if p.len() < 3 {
        return p;
    }
    let croix = |o: Pt, a: Pt, b: Pt| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let mut bas: Vec<Pt> = Vec::new();
    for &q in &p {
        while bas.len() >= 2 && croix(bas[bas.len() - 2], bas[bas.len() - 1], q) <= 0.0 {
            bas.pop();
        }
        bas.push(q);
    }
    let mut haut: Vec<Pt> = Vec::new();
    for &q in p.iter().rev() {
        while haut.len() >= 2 && croix(haut[haut.len() - 2], haut[haut.len() - 1], q) <= 0.0 {
            haut.pop();
        }
        haut.push(q);
    }
    bas.pop();
    haut.pop();
    bas.extend(haut);
    bas
}
